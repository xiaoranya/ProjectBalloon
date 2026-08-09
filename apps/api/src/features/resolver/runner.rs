use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use tokio::sync::watch;
use tracing::{info, warn};
use uuid::Uuid;

pub struct ResolverAutoRunner {
    database: PgPool,
}

impl ResolverAutoRunner {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        info!("Resolver auto-play runner started");
        loop {
            if *shutdown.borrow() {
                break;
            }
            match self.advance_due().await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => warn!(%error, "Resolver auto-play advance failed"),
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                result = shutdown.changed() => {
                    if result.is_err() || *shutdown.borrow() { break; }
                }
            }
        }
        info!("Resolver auto-play runner stopped");
    }

    pub(crate) async fn advance_due(&self) -> Result<bool, sqlx::Error> {
        let mut tx = self.database.begin().await?;
        let advanced = sqlx::query_as::<_, (i64, i64, bool, i32, i64)>(
            r#"
            WITH candidate AS (
                SELECT run.id FROM resolver_runs run
                JOIN contests contest ON contest.id = run.contest_id AND contest.deleted_at IS NULL
                WHERE run.auto_play_enabled AND run.status = 'RUNNING' AND run.next_auto_at <= now()
                ORDER BY run.next_auto_at, run.id FOR UPDATE OF run SKIP LOCKED LIMIT 1
            )
            UPDATE resolver_runs run SET current_step = run.current_step + 1,
                auto_play_enabled = run.current_step + 1 < run.total_steps,
                next_auto_at = CASE WHEN run.current_step + 1 < run.total_steps
                    THEN now() + run.auto_play_interval_ms * interval '1 millisecond' ELSE NULL END,
                updated_at = now(), version = version + 1
            FROM candidate WHERE run.id = candidate.id
            RETURNING run.id, run.contest_id, run.official, run.current_step,
                coalesce(run.created_by_user_id, 0)
            "#,
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some((run_id, contest_id, official, step, actor)) = advanced else {
            tx.rollback().await?;
            return Ok(false);
        };
        sqlx::query(
            r#"
            UPDATE resolver_current_state current SET step_index = snapshot.step_index,
                state_data = snapshot.state_data, state_sha256 = snapshot.state_sha256,
                updated_at = now(), version = current.version + 1
            FROM resolver_snapshots snapshot
            WHERE current.run_id = $1 AND snapshot.run_id = current.run_id
                AND snapshot.step_index = $2
            "#,
        )
        .bind(run_id)
        .bind(step)
        .execute(&mut *tx)
        .await?;
        let sequence = sqlx::query_scalar::<_, i32>(
            "SELECT coalesce(max(sequence), -1) + 1 FROM resolver_events WHERE run_id = $1",
        )
        .bind(run_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO resolver_events (run_id, event_type, payload, sequence, actor_user_id) VALUES ($1, 'AUTO_NEXT', $2, $3, nullif($4, 0))")
            .bind(run_id).bind(json!({"stepIndex": step}).to_string()).bind(sequence).bind(actor)
            .execute(&mut *tx).await?;
        sqlx::query("INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, payload_json) VALUES ($1, $2, 'RESOLVER_STATE_CHANGED', $3, $4)")
            .bind(Uuid::new_v4()).bind(contest_id).bind(if official { "PUBLIC" } else { "STAFF" })
            .bind(json!({"resolverRunId": run_id, "action": "AUTO_NEXT", "stepIndex": step, "status": "RUNNING"}))
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }
}
