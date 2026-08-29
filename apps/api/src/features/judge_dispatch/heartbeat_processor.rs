use project_balloon_contracts::WorkerHeartbeat;
use sqlx::PgPool;

/// Separates PostgreSQL failures from the local serialization/range failures
/// of a heartbeat payload instead of disguising them as `sqlx::Error`.
#[derive(Debug, thiserror::Error)]
pub enum HeartbeatProcessError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error("heartbeat {0} does not fit smallint: {1}")]
    SmallintRange(&'static str, #[source] std::num::TryFromIntError),
}

#[derive(Clone)]
pub struct WorkerHeartbeatProcessor {
    database: PgPool,
}

impl WorkerHeartbeatProcessor {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn apply(&self, heartbeat: &WorkerHeartbeat) -> Result<(), HeartbeatProcessError> {
        let languages = serde_json::to_value(&heartbeat.languages)?;
        let runtime_versions = serde_json::to_value(&heartbeat.runtime_versions)?;
        sqlx::query(
            r#"
            INSERT INTO judge_workers (
                worker_id, instance_id, started_at, last_seen_at, capacity, active_tasks,
                languages, runtime_versions, sandbox_runtime, last_message_id, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
            ON CONFLICT (worker_id) DO UPDATE SET
                instance_id = EXCLUDED.instance_id,
                started_at = EXCLUDED.started_at,
                last_seen_at = EXCLUDED.last_seen_at,
                capacity = EXCLUDED.capacity,
                active_tasks = EXCLUDED.active_tasks,
                languages = EXCLUDED.languages,
                runtime_versions = EXCLUDED.runtime_versions,
                sandbox_runtime = EXCLUDED.sandbox_runtime,
                last_message_id = EXCLUDED.last_message_id,
                updated_at = now()
            WHERE EXCLUDED.last_seen_at >= judge_workers.last_seen_at
            "#,
        )
        .bind(&heartbeat.worker_id)
        .bind(heartbeat.instance_id)
        .bind(heartbeat.started_at)
        .bind(heartbeat.occurred_at)
        .bind(
            i16::try_from(heartbeat.capacity)
                .map_err(|error| HeartbeatProcessError::SmallintRange("capacity", error))?,
        )
        .bind(
            i16::try_from(heartbeat.active_tasks).map_err(|error| {
                HeartbeatProcessError::SmallintRange("active task count", error)
            })?,
        )
        .bind(languages)
        .bind(runtime_versions)
        .bind(&heartbeat.sandbox_runtime)
        .bind(heartbeat.message_id)
        .execute(&self.database)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use project_balloon_contracts::{WORKER_HEARTBEAT_SCHEMA_VERSION, WorkerHeartbeat};
    use sqlx::PgPool;
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    use crate::features::judge_dispatch::heartbeat_processor::WorkerHeartbeatProcessor;

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn latest_heartbeat_wins_and_stale_delivery_cannot_regress_capacity(pool: PgPool) {
        let now = OffsetDateTime::now_utc();
        let mut heartbeat = WorkerHeartbeat {
            schema_version: WORKER_HEARTBEAT_SCHEMA_VERSION,
            message_id: Uuid::new_v4(),
            worker_id: "heartbeat-test-worker".to_owned(),
            instance_id: Uuid::new_v4(),
            started_at: now - Duration::MINUTE,
            occurred_at: now,
            capacity: 4,
            active_tasks: 2,
            languages: vec!["c".to_owned(), "cpp".to_owned()],
            runtime_versions: BTreeMap::from([("cpp".to_owned(), "12.2.0".to_owned())]),
            sandbox_runtime: Some("runsc".to_owned()),
        };
        let processor = WorkerHeartbeatProcessor::new(pool.clone());
        processor.apply(&heartbeat).await.expect("apply current heartbeat");

        heartbeat.message_id = Uuid::new_v4();
        heartbeat.occurred_at = now - Duration::SECOND;
        heartbeat.capacity = 1;
        heartbeat.active_tasks = 0;
        processor.apply(&heartbeat).await.expect("ignore stale heartbeat");

        let persisted = sqlx::query_as::<_, (i16, i16, OffsetDateTime)>(
            "SELECT capacity, active_tasks, last_seen_at FROM judge_workers WHERE worker_id = $1",
        )
        .bind(&heartbeat.worker_id)
        .fetch_one(&pool)
        .await
        .expect("load Worker heartbeat");
        assert_eq!((persisted.0, persisted.1), (4, 2));
        assert_eq!(persisted.2.unix_timestamp_nanos() / 1_000, now.unix_timestamp_nanos() / 1_000);
    }
}
