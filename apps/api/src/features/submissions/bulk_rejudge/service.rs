use sqlx::PgPool;

use crate::{error::AppError, features::auth::model::AuthUser};

use super::model::{
    BatchRejudgeCreateRequest, BatchRejudgeFilter, BatchRejudgeItemResponse,
    BatchRejudgePreviewResponse, BatchRejudgeTaskResponse, BatchRejudgeTaskRow,
};

#[derive(Clone)]
pub struct BatchRejudgeService {
    database: PgPool,
}

impl BatchRejudgeService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn preview(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        filter: BatchRejudgeFilter,
    ) -> Result<BatchRejudgePreviewResponse, AppError> {
        require_access(&self.database, contest_id, actor).await?;
        let filter = filter.validate()?;
        Ok(BatchRejudgePreviewResponse {
            matched_submissions: count_matches(&self.database, contest_id, &filter).await?,
        })
    }

    pub async fn create(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        request: BatchRejudgeCreateRequest,
    ) -> Result<BatchRejudgeTaskResponse, AppError> {
        require_access(&self.database, contest_id, actor).await?;
        let request = request.validate()?;
        if let Some((existing_id, existing_contest_id, existing_creator_id)) =
            sqlx::query_as::<_, (i64, i64, i64)>(
            "SELECT id, contest_id, created_by_user_id FROM batch_rejudge_tasks WHERE idempotency_key = $1",
        )
        .bind(&request.idempotency_key)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load idempotent batch rejudge", error))?
        {
            if existing_contest_id == contest_id && existing_creator_id == actor.id {
                return self.get(contest_id, existing_id, actor).await;
            }
            return Err(AppError::conflict(
                "IDEMPOTENCY_KEY_REUSED",
                "Idempotency key is already used by another batch rejudge request",
            ));
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin batch rejudge", error))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&request.idempotency_key)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("lock batch rejudge idempotency key", error))?;
        if let Some((existing_id, existing_contest_id, existing_creator_id)) =
            sqlx::query_as::<_, (i64, i64, i64)>(
                "SELECT id, contest_id, created_by_user_id FROM batch_rejudge_tasks WHERE idempotency_key = $1",
            )
            .bind(&request.idempotency_key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("recheck batch rejudge idempotency key", error))?
        {
            transaction
                .rollback()
                .await
                .map_err(|error| AppError::internal("rollback idempotent batch rejudge", error))?;
            if existing_contest_id == contest_id && existing_creator_id == actor.id {
                return self.get(contest_id, existing_id, actor).await;
            }
            return Err(AppError::conflict(
                "IDEMPOTENCY_KEY_REUSED",
                "Idempotency key is already used by another batch rejudge request",
            ));
        }
        let filter_data = serde_json::to_string(&request.filter)
            .map_err(|error| AppError::internal("encode batch rejudge filter", error))?;
        let task_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO batch_rejudge_tasks
                (contest_id, status, idempotency_key, filter_data, total_items, created_by_user_id)
            VALUES ($1, 'PENDING', $2, $3, $4, $5)
            RETURNING id
            "#,
        )
        .bind(contest_id)
        .bind(&request.idempotency_key)
        .bind(filter_data)
        .bind(request.expected_count)
        .bind(actor.id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("insert batch rejudge task", error))?;
        let inserted = insert_items(&mut transaction, task_id, contest_id, &request.filter).await?;
        if inserted != request.expected_count {
            return Err(AppError::conflict(
                "BATCH_REJUDGE_COUNT_CHANGED",
                "Matched submission set changed; preview and confirm again",
            ));
        }
        sqlx::query(
            r#"
            INSERT INTO audit_logs
                (actor_user_id, action, target_type, target_id, result)
            VALUES ($1, 'BATCH_REJUDGE_CREATED', 'BATCH_REJUDGE_TASK', $2, 'success')
            "#,
        )
        .bind(actor.id)
        .bind(task_id.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("record batch rejudge audit", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit batch rejudge", error))?;
        self.get(contest_id, task_id, actor).await
    }

    pub async fn get(
        &self,
        contest_id: i64,
        task_id: i64,
        actor: &AuthUser,
    ) -> Result<BatchRejudgeTaskResponse, AppError> {
        require_access(&self.database, contest_id, actor).await?;
        let mut task = load_task(&self.database, contest_id, task_id).await?;
        task.items = sqlx::query_as::<_, BatchRejudgeItemResponse>(
            r#"
            SELECT id, submission_id, status, old_judgement_id, new_judgement_id,
                   error_message, attempts, processed_at
            FROM batch_rejudge_items WHERE task_id = $1 ORDER BY id LIMIT 1000
            "#,
        )
        .bind(task_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("load batch rejudge items", error))?;
        task.items_truncated = task.total_items > 1000;
        Ok(task)
    }

    pub async fn list(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<BatchRejudgeTaskResponse>, AppError> {
        require_access(&self.database, contest_id, actor).await?;
        let rows = sqlx::query_as::<_, BatchRejudgeTaskRow>(safe_sql!(
            "{} WHERE contest_id = $1 ORDER BY created_at DESC, id DESC LIMIT 100",
            TASK_COLUMNS
        ))
        .bind(contest_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list batch rejudge tasks", error))?;
        Ok(rows.into_iter().map(BatchRejudgeTaskRow::response).collect())
    }

    pub async fn pause(
        &self,
        contest_id: i64,
        task_id: i64,
        actor: &AuthUser,
    ) -> Result<BatchRejudgeTaskResponse, AppError> {
        require_access(&self.database, contest_id, actor).await?;
        let changed = sqlx::query(
            r#"
            UPDATE batch_rejudge_tasks
            SET status = 'PAUSED', cancel_requested = true, updated_at = now(), version = version + 1
            WHERE id = $1 AND contest_id = $2 AND status IN ('PENDING', 'RUNNING')
            "#,
        )
        .bind(task_id)
        .bind(contest_id)
        .execute(&self.database)
        .await
        .map_err(|error| AppError::internal("pause batch rejudge", error))?;
        if changed.rows_affected() == 0 {
            load_task(&self.database, contest_id, task_id).await?;
            return Err(AppError::conflict(
                "BATCH_REJUDGE_STATE_CHANGED",
                "Batch rejudge task cannot be paused in its current state",
            ));
        }
        self.get(contest_id, task_id, actor).await
    }

    pub async fn resume(
        &self,
        contest_id: i64,
        task_id: i64,
        actor: &AuthUser,
    ) -> Result<BatchRejudgeTaskResponse, AppError> {
        require_access(&self.database, contest_id, actor).await?;
        let changed = sqlx::query(
            r#"
            UPDATE batch_rejudge_tasks
            SET status = 'RUNNING', cancel_requested = false, completed_at = NULL,
                updated_at = now(), version = version + 1
            WHERE id = $1 AND contest_id = $2 AND status = 'PAUSED'
            "#,
        )
        .bind(task_id)
        .bind(contest_id)
        .execute(&self.database)
        .await
        .map_err(|error| AppError::internal("resume batch rejudge", error))?;
        if changed.rows_affected() == 0 {
            load_task(&self.database, contest_id, task_id).await?;
            return Err(AppError::conflict(
                "BATCH_REJUDGE_STATE_CHANGED",
                "Batch rejudge task cannot be resumed in its current state",
            ));
        }
        self.get(contest_id, task_id, actor).await
    }
}

const TASK_COLUMNS: &str = r#"
    SELECT id, contest_id, status, total_items, processed_items, succeeded_items,
           failed_items, cancel_requested, created_by_user_id, started_at, completed_at,
           created_at, updated_at
    FROM batch_rejudge_tasks
"#;

async fn load_task(
    database: &PgPool,
    contest_id: i64,
    task_id: i64,
) -> Result<BatchRejudgeTaskResponse, AppError> {
    sqlx::query_as::<_, BatchRejudgeTaskRow>(safe_sql!(
        "{} WHERE id = $1 AND contest_id = $2",
        TASK_COLUMNS
    ))
    .bind(task_id)
    .bind(contest_id)
    .fetch_optional(database)
    .await
    .map_err(|error| AppError::internal("load batch rejudge task", error))?
    .ok_or_else(batch_not_found)
    .map(BatchRejudgeTaskRow::response)
}

async fn require_access(
    database: &PgPool,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if contest_id <= 0 {
        return Err(batch_not_found());
    }
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contests WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(contest_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check batch rejudge contest", error))?;
    if !active {
        return Err(batch_not_found());
    }
    if actor.is_super_admin() {
        return Ok(());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_management_assignments WHERE contest_id = $1 AND user_id = $2)",
    )
    .bind(contest_id)
    .bind(actor.id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check batch rejudge scope", error))?;
    if assigned { Ok(()) } else { Err(batch_not_found()) }
}

async fn count_matches(
    database: &PgPool,
    contest_id: i64,
    filter: &BatchRejudgeFilter,
) -> Result<i32, AppError> {
    let count = sqlx::query_scalar::<_, i64>(safe_sql!("SELECT count(*) {MATCHING_SUBMISSIONS}"))
        .bind(contest_id)
        .bind(filter.problem_id)
        .bind(filter.team_id)
        .bind(filter.language.as_deref())
        .bind(filter.verdict.as_deref())
        .bind(filter.submitted_from)
        .bind(filter.submitted_to)
        .fetch_one(database)
        .await
        .map_err(|error| AppError::internal("preview batch rejudge", error))?;
    i32::try_from(count).map_err(|error| AppError::internal("convert batch match count", error))
}

const MATCHING_SUBMISSIONS: &str = r#"
    FROM submissions submission
    JOIN contests contest ON contest.id = submission.contest_id
                         AND contest.deleted_at IS NULL
    JOIN judgements judgement
      ON judgement.submission_id = submission.id
     AND judgement.active_marker IS TRUE
     AND judgement.completed_at IS NOT NULL
    WHERE submission.contest_id = $1
      AND ($2::bigint IS NULL OR submission.problem_id = $2)
      AND ($3::bigint IS NULL OR submission.team_id = $3)
      AND ($4::text IS NULL OR submission.language = $4)
      AND ($5::text IS NULL OR judgement.verdict = $5)
      AND ($6::timestamptz IS NULL OR submission.submitted_at >= $6)
      AND ($7::timestamptz IS NULL OR submission.submitted_at <= $7)
"#;

async fn insert_items(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    task_id: i64,
    contest_id: i64,
    filter: &BatchRejudgeFilter,
) -> Result<i32, AppError> {
    let result = sqlx::query(safe_sql!(
        "INSERT INTO batch_rejudge_items (task_id, submission_id, status, old_judgement_id) SELECT $8, submission.id, 'PENDING', judgement.id {MATCHING_SUBMISSIONS}"
    ))
    .bind(contest_id)
    .bind(filter.problem_id)
    .bind(filter.team_id)
    .bind(filter.language.as_deref())
    .bind(filter.verdict.as_deref())
    .bind(filter.submitted_from)
    .bind(filter.submitted_to)
    .bind(task_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("insert batch rejudge items", error))?;
    i32::try_from(result.rows_affected())
        .map_err(|error| AppError::internal("convert batch rejudge item count", error))
}

fn batch_not_found() -> AppError {
    AppError::not_found("BATCH_REJUDGE_NOT_FOUND", "Batch rejudge task was not found")
}
