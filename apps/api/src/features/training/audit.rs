use std::net::IpAddr;

use sqlx::{Executor, Postgres};

use crate::error::AppError;

/// Records an administrative change made through the training endpoints.
///
/// Problem publication, editorials, practice settings and training sets all
/// change what contestants see, but none of them wrote an `audit_logs` row,
/// so an incident review had no way to tell who published a problem or who
/// relaxed the daily submission limit.
pub(crate) async fn record_admin_audit<'e, E>(
    executor: E,
    actor_user_id: i64,
    action: &'static str,
    target_type: &'static str,
    target_id: i64,
    request_ip: IpAddr,
) -> Result<(), AppError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES ($1, $2, $3, $4, $5, 'success')
        "#,
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id.to_string())
    .bind(request_ip.to_string())
    .execute(executor)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("record training audit", error))
}
