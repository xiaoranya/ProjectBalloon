use sqlx::PgPool;

use crate::{error::AppError, pagination::PageResponse};

use crate::features::audit_logs::model::{AuditLogResponse, ValidatedAuditLogQuery};

pub struct AuditLogService {
    database: PgPool,
}

impl AuditLogService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn list(
        &self,
        query: ValidatedAuditLogQuery,
    ) -> Result<PageResponse<AuditLogResponse>, AppError> {
        let total_elements = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT count(*)
            FROM audit_logs
            WHERE ($1::bigint IS NULL OR actor_user_id = $1)
              AND ($2::text IS NULL OR lower(action) LIKE $2 ESCAPE '\')
              AND ($3::text IS NULL OR lower(result) = $3)
              AND ($4::timestamptz IS NULL OR created_at >= $4)
              AND ($5::timestamptz IS NULL OR created_at <= $5)
            "#,
        )
        .bind(query.actor_user_id)
        .bind(&query.action_pattern)
        .bind(&query.result)
        .bind(query.from)
        .bind(query.to)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("count audit logs", error))?;
        let content = sqlx::query_as::<_, AuditLogResponse>(
            r#"
            SELECT
                id,
                actor_user_id,
                action,
                target_type,
                target_id,
                request_ip,
                result,
                created_at
            FROM audit_logs
            WHERE ($1::bigint IS NULL OR actor_user_id = $1)
              AND ($2::text IS NULL OR lower(action) LIKE $2 ESCAPE '\')
              AND ($3::text IS NULL OR lower(result) = $3)
              AND ($4::timestamptz IS NULL OR created_at >= $4)
              AND ($5::timestamptz IS NULL OR created_at <= $5)
            ORDER BY created_at DESC, id DESC
            LIMIT $6 OFFSET $7
            "#,
        )
        .bind(query.actor_user_id)
        .bind(&query.action_pattern)
        .bind(&query.result)
        .bind(query.from)
        .bind(query.to)
        .bind(i64::from(query.size))
        .bind(query.offset)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list audit logs", error))?;
        Ok(PageResponse::new(content, query.page, query.size, total_elements))
    }
}
