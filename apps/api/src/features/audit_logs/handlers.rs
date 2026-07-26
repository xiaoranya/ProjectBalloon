use axum::{
    Json,
    extract::{Query, State},
};

use crate::{
    error::AppError, features::auth::SuperAdminContext, pagination::PageResponse, state::AppState,
};

use super::model::{AuditLogQuery, AuditLogResponse};

#[utoipa::path(get, path = "/api/admin/audit-logs", operation_id = "listAuditLogs", tag = "audit-logs", params(("actorUserId" = Option<i64>, Query), ("action" = Option<String>, Query), ("result" = Option<String>, Query), ("from" = Option<String>, Query), ("to" = Option<String>, Query), ("page" = Option<u32>, Query), ("size" = Option<u32>, Query), ("sort" = Option<String>, Query)), responses((status = 200, body = PageResponse<AuditLogResponse>), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list(
    _context: SuperAdminContext,
    State(state): State<AppState>,
    query: Result<Query<AuditLogQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<PageResponse<AuditLogResponse>>, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "must contain valid audit filters"))?;
    Ok(Json(state.audit_logs().list(query.validate()?).await?))
}
