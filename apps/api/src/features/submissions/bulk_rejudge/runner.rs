use std::{str::FromStr, time::Duration};

use sqlx::PgPool;
use tokio::sync::watch;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
    features::submissions::SubmissionService,
};

pub struct BatchRejudgeRunner {
    database: PgPool,
    submissions: SubmissionService,
    instance_id: Uuid,
}

#[derive(sqlx::FromRow)]
pub(super) struct ClaimedItem {
    pub(super) id: i64,
    pub(super) task_id: i64,
    pub(super) contest_id: i64,
    pub(super) submission_id: i64,
    pub(super) old_judgement_id: Uuid,
    pub(super) created_by_user_id: i64,
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

    pub(super) async fn claim(&self) -> Result<Option<ClaimedItem>, sqlx::Error> {
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

    pub(super) async fn process(&self, item: ClaimedItem) {
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

    pub(super) async fn finish_item(
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
               coalesce(array_agg(permission.code ORDER BY permission.code) FILTER (WHERE permission.code IS NOT NULL), ARRAY[]::varchar[])
        FROM users account
        LEFT JOIN user_permissions membership ON membership.user_id = account.id
        LEFT JOIN permissions permission ON permission.id = membership.permission_id
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
        permissions: row.4,
    })
}
