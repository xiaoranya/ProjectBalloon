use std::{str::FromStr, time::Duration};

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::OffsetDateTime;
use tokio::sync::watch;
use tracing::{info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
};

use super::service::SubmissionService;

const MAX_BATCH_ITEMS: i32 = 10_000;

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchRejudgeFilter {
    pub problem_id: Option<i64>,
    pub team_id: Option<i64>,
    pub language: Option<String>,
    pub verdict: Option<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub submitted_from: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub submitted_to: Option<OffsetDateTime>,
}

impl BatchRejudgeFilter {
    fn validate(mut self) -> Result<Self, AppError> {
        if self.problem_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("problemId", "must be positive"));
        }
        if self.team_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("teamId", "must be positive"));
        }
        if self.submitted_from.zip(self.submitted_to).is_some_and(|(from, to)| from > to) {
            return Err(AppError::validation("submittedFrom", "must not be after submittedTo"));
        }
        self.language = self.language.map(|value| value.trim().to_ascii_lowercase());
        if self
            .language
            .as_ref()
            .is_some_and(|value| !matches!(value.as_str(), "c" | "cpp" | "java" | "python"))
        {
            return Err(AppError::validation("language", "must be c, cpp, java, or python"));
        }
        self.verdict = self.verdict.map(|value| value.trim().to_ascii_uppercase());
        if self.verdict.as_ref().is_some_and(|value| {
            !matches!(
                value.as_str(),
                "ACCEPTED"
                    | "WRONG_ANSWER"
                    | "COMPILE_ERROR"
                    | "RUNTIME_ERROR"
                    | "TIME_LIMIT_EXCEEDED"
                    | "MEMORY_LIMIT_EXCEEDED"
                    | "OUTPUT_LIMIT_EXCEEDED"
                    | "SYSTEM_ERROR"
                    | "CANCELLED"
            )
        }) {
            return Err(AppError::validation("verdict", "contains an unsupported final verdict"));
        }
        Ok(self)
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchRejudgeCreateRequest {
    pub filter: BatchRejudgeFilter,
    pub expected_count: i32,
    pub confirmation_text: String,
    pub idempotency_key: String,
}

impl BatchRejudgeCreateRequest {
    fn validate(mut self) -> Result<Self, AppError> {
        self.filter = self.filter.validate()?;
        if !(1..=MAX_BATCH_ITEMS).contains(&self.expected_count) {
            return Err(AppError::validation("expectedCount", "must be between 1 and 10000"));
        }
        if self.confirmation_text != format!("REJUDGE {}", self.expected_count) {
            return Err(AppError::validation(
                "confirmationText",
                "must equal REJUDGE followed by expectedCount",
            ));
        }
        self.idempotency_key = self.idempotency_key.trim().to_owned();
        if !(8..=128).contains(&self.idempotency_key.len()) {
            return Err(AppError::validation("idempotencyKey", "must contain 8 to 128 bytes"));
        }
        Ok(self)
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchRejudgePreviewResponse {
    pub matched_submissions: i32,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BatchRejudgeItemResponse {
    pub id: i64,
    pub submission_id: i64,
    pub status: String,
    pub old_judgement_id: Option<Uuid>,
    pub new_judgement_id: Option<Uuid>,
    pub error_message: Option<String>,
    pub attempts: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    pub processed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchRejudgeTaskResponse {
    pub id: i64,
    pub contest_id: i64,
    pub status: String,
    pub total_items: i32,
    pub processed_items: i32,
    pub succeeded_items: i32,
    pub failed_items: i32,
    pub cancel_requested: bool,
    pub created_by_user_id: i64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub started_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub completed_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    pub items: Vec<BatchRejudgeItemResponse>,
    pub items_truncated: bool,
}

#[derive(sqlx::FromRow)]
struct BatchRejudgeTaskRow {
    id: i64,
    contest_id: i64,
    status: String,
    total_items: i32,
    processed_items: i32,
    succeeded_items: i32,
    failed_items: i32,
    cancel_requested: bool,
    created_by_user_id: i64,
    started_at: Option<OffsetDateTime>,
    completed_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl BatchRejudgeTaskRow {
    fn response(self) -> BatchRejudgeTaskResponse {
        BatchRejudgeTaskResponse {
            id: self.id,
            contest_id: self.contest_id,
            status: self.status,
            total_items: self.total_items,
            processed_items: self.processed_items,
            succeeded_items: self.succeeded_items,
            failed_items: self.failed_items,
            cancel_requested: self.cancel_requested,
            created_by_user_id: self.created_by_user_id,
            started_at: self.started_at,
            completed_at: self.completed_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            items: Vec::new(),
            items_truncated: false,
        }
    }
}

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
    if actor.has_role("SUPER_ADMIN") {
        return Ok(());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_admin_assignments WHERE contest_id = $1 AND user_id = $2)",
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

pub struct BatchRejudgeRunner {
    database: PgPool,
    submissions: SubmissionService,
    instance_id: Uuid,
}

#[derive(sqlx::FromRow)]
struct ClaimedItem {
    id: i64,
    task_id: i64,
    contest_id: i64,
    submission_id: i64,
    old_judgement_id: Uuid,
    created_by_user_id: i64,
}

impl BatchRejudgeRunner {
    #[must_use]
    pub fn new(database: PgPool) -> Self {
        Self {
            submissions: SubmissionService::new(database.clone()),
            database,
            instance_id: Uuid::new_v4(),
        }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        info!(instance_id = %self.instance_id, "batch rejudge runner started");
        loop {
            if *shutdown.borrow() {
                break;
            }
            match self.claim().await {
                Ok(Some(item)) => self.process(item).await,
                Ok(None) => {
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_millis(250)) => {}
                        changed = shutdown.changed() => {
                            if changed.is_err() || *shutdown.borrow() { break; }
                        }
                    }
                }
                Err(error) => {
                    warn!(%error, "batch rejudge claim failed");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
        info!(instance_id = %self.instance_id, "batch rejudge runner stopped");
    }

    async fn claim(&self) -> Result<Option<ClaimedItem>, sqlx::Error> {
        let item = sqlx::query_as::<_, ClaimedItem>(
            r#"
            WITH candidate AS (
                SELECT item.id
                FROM batch_rejudge_items item
                JOIN batch_rejudge_tasks task ON task.id = item.task_id
                WHERE task.status IN ('PENDING', 'RUNNING')
                  AND task.cancel_requested = false
                  AND (item.status = 'PENDING'
                       OR (item.status = 'PROCESSING' AND item.lease_until < now()))
                ORDER BY task.created_at, item.id
                LIMIT 1
                FOR UPDATE OF item SKIP LOCKED
            ), claimed AS (
                UPDATE batch_rejudge_items item
                SET status = 'PROCESSING', attempts = item.attempts + 1,
                    lease_owner = $1, lease_until = now() + interval '30 seconds'
                FROM candidate
                WHERE item.id = candidate.id
                RETURNING item.id, item.task_id, item.submission_id, item.old_judgement_id
            )
            SELECT claimed.id, claimed.task_id, task.contest_id, claimed.submission_id,
                   claimed.old_judgement_id, task.created_by_user_id
            FROM claimed JOIN batch_rejudge_tasks task ON task.id = claimed.task_id
            "#,
        )
        .bind(self.instance_id)
        .fetch_optional(&self.database)
        .await?;
        if let Some(item) = &item {
            sqlx::query(
                "UPDATE batch_rejudge_tasks SET status = 'RUNNING', started_at = coalesce(started_at, now()), updated_at = now() WHERE id = $1 AND status = 'PENDING'",
            )
            .bind(item.task_id)
            .execute(&self.database)
            .await?;
        }
        Ok(item)
    }

    async fn process(&self, item: ClaimedItem) {
        let actor = load_actor(&self.database, item.created_by_user_id).await;
        let outcome = match actor {
            Ok(actor) => {
                self.submissions
                    .rejudge_batch_item(
                        item.contest_id,
                        item.submission_id,
                        item.old_judgement_id,
                        &actor,
                        item.id,
                    )
                    .await
            }
            Err(error) => Err(error),
        };
        let (status, new_judgement_id, error_message) = match outcome {
            Ok(response) => ("SUCCEEDED", Some(response.judgement_id), None),
            Err(error) => ("FAILED", None, Some(format!("{error:?}"))),
        };
        if let Err(error) = self.finish_item(&item, status, new_judgement_id, error_message).await {
            warn!(%error, item_id = item.id, "failed to persist batch rejudge item outcome");
        }
    }

    async fn finish_item(
        &self,
        item: &ClaimedItem,
        status: &str,
        new_judgement_id: Option<Uuid>,
        error_message: Option<String>,
    ) -> Result<(), sqlx::Error> {
        let mut transaction = self.database.begin().await?;
        let updated = sqlx::query_scalar::<_, i64>(
            r#"
            UPDATE batch_rejudge_items
            SET status = $3, new_judgement_id = $4, error_message = $5,
                processed_at = now(), lease_owner = NULL, lease_until = NULL
            WHERE id = $1 AND status = 'PROCESSING' AND lease_owner = $2
            RETURNING task_id
            "#,
        )
        .bind(item.id)
        .bind(self.instance_id)
        .bind(status)
        .bind(new_judgement_id)
        .bind(error_message.map(|value| value.chars().take(1000).collect::<String>()))
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(task_id) = updated {
            sqlx::query(
                r#"
                UPDATE batch_rejudge_tasks task
                SET processed_items = aggregate.processed,
                    succeeded_items = aggregate.succeeded,
                    failed_items = aggregate.failed,
                    status = CASE WHEN aggregate.processed = task.total_items THEN 'COMPLETED' ELSE task.status END,
                    completed_at = CASE WHEN aggregate.processed = task.total_items THEN now() ELSE NULL END,
                    updated_at = now(), version = version + 1
                FROM (
                    SELECT count(*) FILTER (WHERE status IN ('SUCCEEDED', 'FAILED'))::integer AS processed,
                           count(*) FILTER (WHERE status = 'SUCCEEDED')::integer AS succeeded,
                           count(*) FILTER (WHERE status = 'FAILED')::integer AS failed
                    FROM batch_rejudge_items WHERE task_id = $1
                ) aggregate
                WHERE task.id = $1
                "#,
            )
            .bind(task_id)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await
    }
}

async fn load_actor(database: &PgPool, user_id: i64) -> Result<AuthUser, AppError> {
    let row = sqlx::query_as::<_, (String, String, String, bool, Vec<String>)>(
        r#"
        SELECT account.username, account.display_name, account.user_type, account.password_reset_required,
               coalesce(array_agg(role.code ORDER BY role.code) FILTER (WHERE role.code IS NOT NULL), ARRAY[]::varchar[])
        FROM users account
        LEFT JOIN user_roles membership ON membership.user_id = account.id
        LEFT JOIN roles role ON role.id = membership.role_id
        WHERE account.id = $1 AND account.enabled = true
        GROUP BY account.id
        "#,
    )
    .bind(user_id)
    .fetch_optional(database)
    .await
    .map_err(|error| AppError::internal("load batch rejudge actor", error))?
    .ok_or_else(|| AppError::forbidden("BATCH_REJUDGE_ACTOR_DISABLED", "Task creator is disabled"))?;
    Ok(AuthUser {
        id: user_id,
        username: row.0,
        display_name: row.1,
        user_type: UserType::from_str(&row.2)?,
        password_reset_required: row.3,
        roles: row.4,
    })
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::{
        BatchRejudgeCreateRequest, BatchRejudgeFilter, BatchRejudgeRunner, BatchRejudgeService,
    };
    use crate::features::{
        auth::model::{AuthUser, UserType},
        submissions::SubmissionService,
    };

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn batch_rejudge_is_persistent_pausable_and_item_idempotent(pool: PgPool) {
        let admin_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users (username, password_hash, display_name, user_type)
            VALUES ('batch-root', 'test-hash', 'Batch Root', 'SUPER_ADMIN') RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert batch administrator");
        let admin = AuthUser {
            id: admin_id,
            username: "batch-root".into(),
            display_name: "Batch Root".into(),
            user_type: UserType::SuperAdmin,
            roles: Vec::new(),
            password_reset_required: false,
        };
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Batch Team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert batch team");
        let contest_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO contests (name, status, visibility, start_at, end_at)
            VALUES ('Batch Contest', 'RUNNING', 'PRIVATE', now() - interval '1 hour', now() + interval '1 hour')
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert batch contest");
        sqlx::query(
            "INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')",
        )
        .bind(contest_id)
        .bind(team_id)
        .execute(&pool)
        .await
        .expect("roster batch team");
        let problem_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO problems
                (slug, title, languages, testdata_version, testdata_object_key, testdata_sha256)
            VALUES ('batch-a', 'Batch A', '["cpp"]', 1, 'problems/batch/v1.zip', $1)
            RETURNING id
            "#,
        )
        .bind("a".repeat(64))
        .fetch_one(&pool)
        .await
        .expect("insert batch problem");
        sqlx::query(
            r#"
            INSERT INTO problem_testdata_versions
                (problem_id, version, object_key, sha256, bytes, case_count)
            VALUES ($1, 1, 'problems/batch/v1.zip', $2, 100, 1)
            "#,
        )
        .bind(problem_id)
        .bind("a".repeat(64))
        .execute(&pool)
        .await
        .expect("insert batch test data");
        sqlx::query(
            "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
        )
        .bind(contest_id)
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("assign batch problem");
        let submission_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO submissions
                (contest_id, problem_id, team_id, language, source_object_key,
                 source_size_bytes, source_sha256, status, judged_at)
            VALUES ($1, $2, $3, 'cpp', 'sources/batch.cpp', 10, $4, 'ACCEPTED', now())
            RETURNING id
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(team_id)
        .bind("b".repeat(64))
        .fetch_one(&pool)
        .await
        .expect("insert batch submission");
        let old_judgement_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO judgements (id, submission_id, verdict, completed_at) VALUES ($1, $2, 'ACCEPTED', now())",
        )
        .bind(old_judgement_id)
        .bind(submission_id)
        .execute(&pool)
        .await
        .expect("insert batch old judgement");

        let service = BatchRejudgeService::new(pool.clone());
        let filter = || BatchRejudgeFilter {
            problem_id: Some(problem_id),
            team_id: None,
            language: Some("cpp".into()),
            verdict: Some("ACCEPTED".into()),
            submitted_from: None,
            submitted_to: None,
        };
        let preview =
            service.preview(contest_id, &admin, filter()).await.expect("preview batch rejudge");
        assert_eq!(preview.matched_submissions, 1);
        let request = || BatchRejudgeCreateRequest {
            filter: filter(),
            expected_count: 1,
            confirmation_text: "REJUDGE 1".into(),
            idempotency_key: "batch-test-key-0001".into(),
        };
        let task = service.create(contest_id, &admin, request()).await.expect("create batch task");
        assert_eq!((task.status.as_str(), task.total_items, task.items.len()), ("PENDING", 1, 1));
        let duplicate =
            service.create(contest_id, &admin, request()).await.expect("idempotent batch create");
        assert_eq!(duplicate.id, task.id);
        let paused = service.pause(contest_id, task.id, &admin).await.expect("pause batch task");
        assert_eq!(paused.status, "PAUSED");
        let resumed = service.resume(contest_id, task.id, &admin).await.expect("resume batch task");
        assert_eq!(resumed.status, "RUNNING");

        let runner = BatchRejudgeRunner::new(pool.clone());
        let item = runner.claim().await.expect("claim batch item").expect("pending batch item");
        let item_id = item.id;
        runner.process(item).await;
        let completed =
            service.get(contest_id, task.id, &admin).await.expect("load completed task");
        assert_eq!(completed.status, "COMPLETED");
        assert_eq!(
            (completed.processed_items, completed.succeeded_items, completed.failed_items),
            (1, 1, 0),
            "batch item error: {:?}",
            completed.items[0].error_message
        );
        let new_judgement_id = completed.items[0].new_judgement_id.expect("new batch judgement");
        let recovered = SubmissionService::new(pool.clone())
            .rejudge_batch_item(contest_id, submission_id, old_judgement_id, &admin, item_id)
            .await
            .expect("recover already committed batch item");
        assert_eq!(recovered.judgement_id, new_judgement_id);
        let anchored = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM judgements WHERE batch_rejudge_item_id = $1",
        )
        .bind(item_id)
        .fetch_one(&pool)
        .await
        .expect("count batch judgement anchors");
        assert_eq!(anchored, 1);
    }
}
