use std::time::Duration;

use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use tokio::sync::watch;
use tracing::warn;

use crate::error::AppError;
use crate::features::realtime::RealtimeOutbox;

// Keep an end timestamp typo recoverable during the normal administrative
// extension window. Explicit transitions remain the only way to reopen a
// contest after automatic finalization.
const AUTOMATIC_END_GRACE_SECONDS: i32 = 60;

pub struct ContestLifecycleRunner {
    database: PgPool,
    poll_interval: Duration,
    outbox: Option<RealtimeOutbox>,
    projection: Option<crate::features::scoreboard::ScoreboardProjection>,
}

impl ContestLifecycleRunner {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database, poll_interval: Duration::from_secs(1), outbox: None, projection: None }
    }

    #[must_use]
    pub fn with_outbox_option(mut self, outbox: Option<RealtimeOutbox>) -> Self {
        self.outbox = outbox;
        self
    }

    #[must_use]
    pub fn with_projection_option(
        mut self,
        projection: Option<crate::features::scoreboard::ScoreboardProjection>,
    ) -> Self {
        self.projection = projection;
        self
    }

    #[cfg(test)]
    fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut ticker = tokio::time::interval(self.poll_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(error) = self.advance_due().await {
                        warn!(?error, "automatic contest lifecycle advance failed");
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() { return; }
                }
            }
        }
    }

    pub async fn advance_due(&self) -> Result<u64, AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin automatic contest lifecycle", error))?;
        let mut changed = 0;
        let started = transition_due(
            &self.outbox,
            &mut transaction,
            "FROZEN_CONFIG",
            "RUNNING",
            "STARTED",
            "start_at",
            "CONTEST_AUTO_STARTED",
        )
        .await?;
        changed += started.len();
        let frozen = record_freezes(&self.outbox, &mut transaction).await?;
        changed += frozen.len();
        let mut ended = transition_due(
            &self.outbox,
            &mut transaction,
            "RUNNING",
            "ENDED",
            "ENDED",
            "end_at",
            "CONTEST_AUTO_ENDED",
        )
        .await?;
        changed += ended.len();
        ended.extend(
            transition_due(
                &self.outbox,
                &mut transaction,
                "PAUSED",
                "ENDED",
                "ENDED",
                "end_at",
                "CONTEST_AUTO_ENDED",
            )
            .await?,
        );
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit automatic contest lifecycle", error))?;
        self.maintain_scoreboard_projection(&started, &frozen, &ended).await;
        Ok(changed as u64)
    }

    /// Contest lifecycle side effects for the Redis scoreboard projection:
    /// warm it on start, snapshot the frozen board at freeze, and backfill the
    /// durable DB projection once the contest ends (running contests no longer
    /// write `contest_scoreboard_cells` per judgement).
    async fn maintain_scoreboard_projection(&self, started: &[i64], frozen: &[i64], ended: &[i64]) {
        let Some(projection) = &self.projection else { return };
        for contest_id in started {
            if let Err(error) = projection.rebuild_contest(&self.database, *contest_id).await {
                warn!(?error, %contest_id, "scoreboard projection warm-up failed");
            }
        }
        for contest_id in frozen {
            if let Err(error) = projection.freeze_snapshot(*contest_id).await {
                warn!(?error, %contest_id, "scoreboard freeze snapshot failed");
            }
        }
        for contest_id in ended {
            if let Err(error) = backfill_cells(&self.database, *contest_id).await {
                warn!(?error, %contest_id, "scoreboard DB backfill after contest end failed");
            }
        }
    }
}

/// Recomputes the durable `contest_scoreboard_cells/rows` for a finished
/// contest from the authoritative submission history (one bounded rescan per
/// cell, once per contest).
async fn backfill_cells(database: &PgPool, contest_id: i64) -> Result<(), AppError> {
    let mut transaction = database
        .begin()
        .await
        .map_err(|error| AppError::internal("begin scoreboard backfill", error))?;
    let pairs = sqlx::query_as::<_, (i64, i64)>(
        "SELECT DISTINCT team_id, problem_id FROM submissions WHERE contest_id = $1",
    )
    .bind(contest_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(|error| AppError::internal("load scoreboard backfill cells", error))?;
    for (team_id, problem_id) in pairs {
        crate::features::scoreboard::rebuild_cell(
            &mut transaction,
            contest_id,
            team_id,
            problem_id,
        )
        .await
        .map_err(|error| AppError::internal("backfill scoreboard cell", error))?;
    }
    transaction
        .commit()
        .await
        .map_err(|error| AppError::internal("commit scoreboard backfill", error))
}

async fn transition_due(
    outbox: &Option<RealtimeOutbox>,
    transaction: &mut Transaction<'_, Postgres>,
    from: &'static str,
    to: &'static str,
    milestone: &'static str,
    timestamp_column: &'static str,
    audit_action: &'static str,
) -> Result<Vec<i64>, AppError> {
    let due_condition = if timestamp_column == "end_at" {
        format!("{timestamp_column} <= now() - interval '{AUTOMATIC_END_GRACE_SECONDS} seconds'")
    } else {
        format!("{timestamp_column} <= now()")
    };
    let sql = format!(
        "SELECT id,{timestamp_column} FROM contests WHERE status=$1 AND deleted_at IS NULL AND {timestamp_column} IS NOT NULL AND {due_condition} ORDER BY {timestamp_column},id FOR UPDATE SKIP LOCKED LIMIT 100"
    );
    let due = sqlx::query_as::<_, (i64, time::OffsetDateTime)>(sqlx::AssertSqlSafe(sql))
        .bind(from)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| AppError::internal("load due contest lifecycle transitions", error))?;
    for (contest_id, scheduled_at) in &due {
        sqlx::query("UPDATE contests SET status=$2,version=version+1,updated_at=now() WHERE id=$1 AND status=$3").bind(contest_id).bind(to).bind(from).execute(&mut **transaction).await.map_err(|error| AppError::internal("advance contest lifecycle", error))?;
        insert_milestone(transaction, *contest_id, milestone, *scheduled_at, from, to).await?;
        automatic_audit(transaction, *contest_id, audit_action, &format!("{from}->{to}")).await?;
        lifecycle_event(
            outbox,
            *contest_id,
            audit_action,
            json!({"from":from,"to":to,"scheduledAt":scheduled_at}),
        )
        .await?;
    }
    Ok(due.into_iter().map(|(contest_id, _)| contest_id).collect())
}

async fn record_freezes(
    outbox: &Option<RealtimeOutbox>,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<Vec<i64>, AppError> {
    let due = sqlx::query_as::<_, (i64, time::OffsetDateTime, String)>(
        r#"
        SELECT c.id,c.freeze_at,c.status
        FROM contests c
        WHERE c.deleted_at IS NULL
            AND c.freeze_at IS NOT NULL
            AND c.freeze_at<=now()
            AND c.status IN('RUNNING','PAUSED','ENDED','ARCHIVED')
            AND NOT EXISTS(
                SELECT 1 FROM contest_lifecycle_milestones m
                WHERE m.contest_id=c.id AND m.milestone='FROZEN'
            )
        ORDER BY c.freeze_at,c.id
        FOR UPDATE OF c SKIP LOCKED
        LIMIT 100
        "#,
    )
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("load due contest freezes", error))?;
    for (contest_id, scheduled_at, status) in &due {
        insert_milestone(transaction, *contest_id, "FROZEN", *scheduled_at, status, status).await?;
        automatic_audit(
            transaction,
            *contest_id,
            "CONTEST_AUTO_FROZEN",
            &format!("{status}->{status}"),
        )
        .await?;
        lifecycle_event(
            outbox,
            *contest_id,
            "CONTEST_AUTO_FROZEN",
            json!({"status":status,"scheduledAt":scheduled_at}),
        )
        .await?;
    }
    Ok(due.into_iter().map(|(contest_id, _, _)| contest_id).collect())
}

async fn insert_milestone(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    milestone: &str,
    scheduled_at: time::OffsetDateTime,
    from: &str,
    to: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO contest_lifecycle_milestones(contest_id,milestone,scheduled_at,previous_status,new_status) VALUES($1,$2,$3,$4,$5) ON CONFLICT(contest_id,milestone) DO NOTHING").bind(contest_id).bind(milestone).bind(scheduled_at).bind(from).bind(to).execute(&mut **transaction).await.map(|_|()).map_err(|error|AppError::internal("record contest lifecycle milestone",error))
}
async fn automatic_audit(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    action: &str,
    result: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs(actor_user_id,action,target_type,target_id,request_ip,result) VALUES(NULL,$1,'CONTEST',$2,'system',$3)").bind(action).bind(contest_id.to_string()).bind(result).execute(&mut **transaction).await.map(|_|()).map_err(|error|AppError::internal("audit automatic contest lifecycle",error))
}
async fn lifecycle_event(
    outbox: &Option<RealtimeOutbox>,
    contest_id: i64,
    event_type: &str,
    payload: serde_json::Value,
) -> Result<(), AppError> {
    crate::features::realtime::outbox::enqueue_optional(
        outbox.as_ref(),
        contest_id,
        event_type,
        "PUBLIC",
        None,
        payload,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn automatic_lifecycle_is_ordered_idempotent_and_multi_instance_safe(pool: PgPool) {
        let contest = sqlx::query_scalar::<_,i64>("INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES('Automatic Lifecycle','FROZEN_CONFIG','PUBLIC',now()-interval '3 hours',now()-interval '2 hours',now()-interval '1 hour') RETURNING id").fetch_one(&pool).await.expect("contest");
        let first =
            ContestLifecycleRunner::new(pool.clone()).with_poll_interval(Duration::from_millis(1));
        let second =
            ContestLifecycleRunner::new(pool.clone()).with_poll_interval(Duration::from_millis(1));
        let (a, b) = tokio::join!(first.advance_due(), second.advance_due());
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM contests WHERE id=$1")
                .bind(contest)
                .fetch_one(&pool)
                .await
                .expect("status"),
            "ENDED"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM contest_lifecycle_milestones WHERE contest_id=$1"
            )
            .bind(contest)
            .fetch_one(&pool)
            .await
            .expect("milestones"),
            3
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_logs WHERE target_id=$1 AND action LIKE 'CONTEST_AUTO_%'").bind(contest.to_string()).fetch_one(&pool).await.expect("audits"),3);
        assert_eq!(
            ContestLifecycleRunner::new(pool.clone()).advance_due().await.expect("repeat"),
            0
        );
    }
}
