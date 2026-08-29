use crate::{error::AppError, features::auth::AuthContext, state::AppState};
use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, QueryBuilder};
use std::collections::HashSet;
use time::OffsetDateTime;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateVirtualSessionRequest {
    title: String,
    duration_minutes: i32,
    problem_ids: Vec<i64>,
}
#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct VirtualSessionResponse {
    id: i64,
    title: String,
    #[serde(with = "time::serde::rfc3339")]
    start_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    end_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    server_time: OffsetDateTime,
    status: String,
    total_problems: i64,
    solved_problems: i64,
}
#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct VirtualProblemResponse {
    problem_id: i64,
    slug: String,
    title: String,
    position: i32,
    solved: bool,
    attempts: i64,
}
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VirtualSessionDetail {
    session: VirtualSessionResponse,
    problems: Vec<VirtualProblemResponse>,
}

const VIRTUAL_SESSION_SQL: &str = r#"
    SELECT s.id,s.title,s.start_at,s.end_at,now() AS server_time,
           CASE WHEN s.archived_at IS NOT NULL THEN 'ARCHIVED'
                WHEN now()<s.start_at THEN 'SCHEDULED'
                WHEN now()<s.end_at THEN 'RUNNING'
                ELSE 'ENDED' END AS status,
           count(DISTINCT i.problem_id)
               FILTER(WHERE problem.id IS NOT NULL AND bank.problem_id IS NOT NULL)::bigint
               AS total_problems,
           count(DISTINCT sub.problem_id)
               FILTER(WHERE sub.status='ACCEPTED' AND problem.id IS NOT NULL
                   AND bank.problem_id IS NOT NULL)::bigint
               AS solved_problems
    FROM practice_virtual_sessions s
    JOIN practice_virtual_items i
        ON i.session_id=s.id
    LEFT JOIN problems problem
        ON problem.id=i.problem_id AND problem.deleted_at IS NULL
    LEFT JOIN problem_bank_entries bank
        ON bank.problem_id=problem.id AND bank.visibility='PUBLIC'
    LEFT JOIN submissions sub
        ON sub.virtual_session_id=s.id AND sub.participant_user_id=s.user_id
"#;

#[utoipa::path(post,path="/api/practice/virtual-sessions",operation_id="createPracticeVirtualSession",tag="practice",request_body=CreateVirtualSessionRequest,responses((status=201,body=VirtualSessionResponse)),security(("session_cookie"=[],"csrf_cookie"=[],"csrf_header"=[])))]
pub async fn create(
    context: AuthContext,
    State(state): State<AppState>,
    payload: Result<Json<CreateVirtualSessionRequest>, JsonRejection>,
) -> Result<(axum::http::StatusCode, Json<VirtualSessionResponse>), AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid virtual session"))?;
    if request.title.trim().is_empty()
        || request.title.len() > 255
        || !(15..=10080).contains(&request.duration_minutes)
        || request.problem_ids.is_empty()
        || request.problem_ids.len() > 50
    {
        return Err(AppError::validation(
            "virtualSession",
            "title, duration, or problem count is invalid",
        ));
    }
    let mut seen = HashSet::new();
    if request.problem_ids.iter().any(|id| *id <= 0 || !seen.insert(*id)) {
        return Err(AppError::validation("problemIds", "must be positive and unique"));
    }
    let public = count_active_public_problems(state.database(), &request.problem_ids)
        .await
        .map_err(|e| AppError::internal("validate virtual problems", e))?;
    if usize::try_from(public).ok() != Some(request.problem_ids.len()) {
        return Err(AppError::conflict(
            "VIRTUAL_PROBLEM_NOT_PUBLIC",
            "Every virtual problem must be public",
        ));
    }
    let mut tx = state
        .database()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin virtual session", e))?;
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO practice_virtual_sessions(user_id,title,start_at,end_at) VALUES($1,$2,now(),now()+make_interval(mins=>$3)) RETURNING id",
    )
    .bind(context.user().id)
    .bind(request.title.trim())
    .bind(request.duration_minutes)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::internal("create virtual session", e))?;
    let mut items = QueryBuilder::<Postgres>::new(
        "INSERT INTO practice_virtual_items(session_id,problem_id,position) ",
    );
    items.push_values(request.problem_ids.iter().enumerate(), |mut bind, (index, problem)| {
        bind.push_bind(id)
            .push_bind(*problem)
            .push_bind(i32::try_from(index + 1).unwrap_or(i32::MAX));
    });
    items
        .build()
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("insert virtual problems", e))?;
    tx.commit().await.map_err(|e| AppError::internal("commit virtual session", e))?;
    Ok((axum::http::StatusCode::CREATED, Json(load_session(&state, id, context.user().id).await?)))
}

#[utoipa::path(get,path="/api/practice/virtual-sessions",operation_id="listPracticeVirtualSessions",tag="practice",responses((status=200,body=[VirtualSessionResponse])),security(("session_cookie"=[])))]
pub async fn list(
    context: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Vec<VirtualSessionResponse>>, AppError> {
    context.require_password_ready()?;
    let sql = format!(
        "{VIRTUAL_SESSION_SQL} WHERE s.user_id=$1 GROUP BY s.id ORDER BY s.created_at DESC,s.id DESC"
    );
    Ok(Json(
        sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(context.user().id)
            .fetch_all(state.database())
            .await
            .map_err(|e| AppError::internal("list virtual sessions", e))?,
    ))
}

#[utoipa::path(get,path="/api/practice/virtual-sessions/{session_id}",operation_id="getPracticeVirtualSession",tag="practice",params(("session_id"=i64,Path)),responses((status=200,body=VirtualSessionDetail)),security(("session_cookie"=[])))]
pub async fn get(
    context: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<VirtualSessionDetail>, AppError> {
    context.require_password_ready()?;
    let session = load_session(&state, id, context.user().id).await?;
    let problems = sqlx::query_as::<_, VirtualProblemResponse>(
        r#"
        SELECT i.problem_id,p.slug,p.title,i.position,
               EXISTS(
                   SELECT 1 FROM submissions s
                   WHERE s.virtual_session_id=i.session_id AND s.problem_id=i.problem_id
                       AND s.participant_user_id=$2 AND s.status='ACCEPTED'
               ) AS solved,
               (
                   SELECT count(*) FROM submissions s
                   WHERE s.virtual_session_id=i.session_id AND s.problem_id=i.problem_id
                       AND s.participant_user_id=$2
               )::bigint AS attempts
        FROM practice_virtual_items i
        JOIN problems p
            ON p.id=i.problem_id AND p.deleted_at IS NULL
        JOIN problem_bank_entries b
            ON b.problem_id=p.id AND b.visibility='PUBLIC'
        WHERE i.session_id=$1
        ORDER BY i.position
        "#,
    )
    .bind(id)
    .bind(context.user().id)
    .fetch_all(state.database())
    .await
    .map_err(|e| AppError::internal("load virtual problems", e))?;
    Ok(Json(VirtualSessionDetail { session, problems }))
}

#[utoipa::path(post,path="/api/practice/virtual-sessions/{session_id}/archive",operation_id="archivePracticeVirtualSession",tag="practice",params(("session_id"=i64,Path)),responses((status=200,body=VirtualSessionResponse),(status=404,body=crate::error::ApiErrorBody)),security(("session_cookie"=[] ,"csrf_cookie"=[] ,"csrf_header"=[])))]
pub async fn archive(
    context: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<VirtualSessionResponse>, AppError> {
    context.require_password_ready()?;
    let changed = sqlx::query(
        "UPDATE practice_virtual_sessions SET archived_at=coalesce(archived_at,now()) WHERE id=$1 AND user_id=$2",
    )
    .bind(id)
    .bind(context.user().id)
    .execute(state.database())
    .await
    .map_err(|e| AppError::internal("archive virtual session", e))?;
    if changed.rows_affected() != 1 {
        return Err(AppError::not_found("VIRTUAL_SESSION_NOT_FOUND", "Virtual session not found"));
    }
    Ok(Json(load_session(&state, id, context.user().id).await?))
}

async fn load_session(
    state: &AppState,
    id: i64,
    user: i64,
) -> Result<VirtualSessionResponse, AppError> {
    let sql = format!("{VIRTUAL_SESSION_SQL} WHERE s.id=$1 AND s.user_id=$2 GROUP BY s.id");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(user)
        .fetch_optional(state.database())
        .await
        .map_err(|e| AppError::internal("load virtual session", e))?
        .ok_or_else(|| {
            AppError::not_found("VIRTUAL_SESSION_NOT_FOUND", "Virtual session not found")
        })
}

async fn count_active_public_problems(
    database: &PgPool,
    problem_ids: &[i64],
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM problem_bank_entries bank JOIN problems problem ON problem.id=bank.problem_id AND problem.deleted_at IS NULL WHERE bank.visibility='PUBLIC' AND bank.problem_id=ANY($1)",
    )
    .bind(problem_ids)
    .fetch_one(database)
    .await
}

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/practice/virtual-sessions", axum::routing::get(list).post(create))
        .route("/api/practice/virtual-sessions/{session_id}", axum::routing::get(get))
        .route("/api/practice/virtual-sessions/{session_id}/archive", axum::routing::post(archive))
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use crate::features::virtual_practice::count_active_public_problems;

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn virtual_sessions_reject_soft_deleted_public_problems(pool: PgPool) {
        let active = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('virtual-active', 'Active') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert active problem");
        let deleted = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('virtual-deleted', 'Deleted') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert deleted problem");
        sqlx::query(
            "INSERT INTO problem_bank_entries (problem_id, visibility, tags, published_at) VALUES ($1, 'PUBLIC', '[]', now()), ($2, 'PUBLIC', '[]', now())",
        )
        .bind(active)
        .bind(deleted)
        .execute(&pool)
        .await
        .expect("insert public problems");
        sqlx::query("UPDATE problems SET deleted_at=now() WHERE id=$1")
            .bind(deleted)
            .execute(&pool)
            .await
            .expect("soft-delete problem");

        assert_eq!(
            count_active_public_problems(&pool, &[active, deleted])
                .await
                .expect("count public problems"),
            1
        );
    }
}
