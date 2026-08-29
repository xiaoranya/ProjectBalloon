use std::time::Duration;

use sqlx::{PgPool, Postgres, Transaction};
use tokio::{sync::watch, time::MissedTickBehavior};
use tracing::{error, info};

use crate::error::AppError;

use crate::features::announcements::service::{public_event_tx, schedule_event_tx};

pub struct AnnouncementScheduleRunner {
    database: PgPool,
    poll_interval: Duration,
}

impl AnnouncementScheduleRunner {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database, poll_interval: Duration::from_secs(1) }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut ticker = tokio::time::interval(self.poll_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => match self.publish_due().await {
                    Ok(changed) if changed > 0 => info!(changed, "processed scheduled announcements"),
                    Ok(_) => {}
                    Err(error) => error!(?error, "scheduled announcement processing failed"),
                },
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() { return; }
                }
            }
        }
    }

    pub async fn publish_due(&self) -> Result<u64, AppError> {
        let mut tx = self.database.begin().await.map_err(|error| {
            AppError::internal("begin scheduled announcement processing", error)
        })?;
        let due = sqlx::query_as::<_, (i64, i64, i64, String)>(
            r#"
            SELECT announcement.id,announcement.contest_id,announcement.created_by,contest.status
            FROM announcements announcement
            JOIN contests contest ON contest.id=announcement.contest_id
            WHERE announcement.status='SCHEDULED' AND announcement.scheduled_at<=now()
            ORDER BY announcement.scheduled_at,announcement.id
            FOR UPDATE OF announcement,contest SKIP LOCKED LIMIT 100
            "#,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| AppError::internal("load due scheduled announcements", error))?;
        for (id, contest_id, created_by, contest_status) in &due {
            if matches!(contest_status.as_str(), "RUNNING" | "PAUSED") {
                sqlx::query("UPDATE announcements SET status='PUBLISHED',published_at=now(),updated_at=now(),version=version+1 WHERE id=$1 AND status='SCHEDULED'")
                    .bind(id).execute(&mut *tx).await
                    .map_err(|error| AppError::internal("publish scheduled announcement", error))?;
                automatic_audit_tx(&mut tx, *created_by, "ANNOUNCEMENT_PUBLISHED", *id).await?;
                public_event_tx(&mut tx, *contest_id, *id, "PUBLISHED").await?;
            } else {
                sqlx::query("UPDATE announcements SET status='CANCELLED',pinned=false,cancelled_at=now(),cancelled_by=$2,updated_at=now(),version=version+1 WHERE id=$1 AND status='SCHEDULED'")
                    .bind(id).bind(created_by).execute(&mut *tx).await
                    .map_err(|error| AppError::internal("cancel expired scheduled announcement", error))?;
                automatic_audit_tx(&mut tx, *created_by, "ANNOUNCEMENT_SCHEDULE_CANCELLED", *id)
                    .await?;
                schedule_event_tx(&mut tx, *contest_id, *id, "CANCELLED").await?;
            }
        }
        tx.commit().await.map_err(|error| {
            AppError::internal("commit scheduled announcement processing", error)
        })?;
        Ok(due.len() as u64)
    }
}

async fn automatic_audit_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: i64,
    action: &str,
    id: i64,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs (actor_user_id,action,target_type,target_id,request_ip,result) VALUES ($1,$2,'ANNOUNCEMENT',$3,'system','success')")
        .bind(actor_id)
        .bind(action)
        .bind(id.to_string())
        .execute(&mut **tx)
        .await
        .map(|_| ())
        .map_err(|error| AppError::internal("record automatic announcement audit", error))
}
