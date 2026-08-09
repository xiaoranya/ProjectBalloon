use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    error::AppError,
    features::{
        auth::{AuthContext, model::AuthUser},
        scoreboard::{ScoreboardQuery, ScoreboardResponse},
    },
    state::AppState,
};

use super::model::ConfigResponse;
use super::service::{audit, require_contest};

#[derive(Debug, Deserialize, ToSchema)]
pub struct PublishedQuery {
    mode: String,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PresentationAnnouncement {
    id: i64,
    title: String,
    body: String,
    pinned: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    published_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublishedPresentation {
    contest_id: i64,
    contest_name: String,
    contest_status: String,
    #[serde(with = "time::serde::rfc3339::option")]
    start_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    freeze_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    end_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    server_time: OffsetDateTime,
    config: ConfigResponse,
    scoreboard: ScoreboardResponse,
    announcements: Vec<PresentationAnnouncement>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastTokenResponse {
    id: i64,
    label: String,
    #[serde(with = "time::serde::rfc3339")]
    expires_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    revoked_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    last_used_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BroadcastTokenRequest {
    label: String,
    #[serde(with = "time::serde::rfc3339")]
    expires_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastTokenCreated {
    id: i64,
    label: String,
    token: String,
    #[serde(with = "time::serde::rfc3339")]
    expires_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CountByName {
    #[sqlx(rename = "name")]
    name: String,
    total: i64,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    #[serde(with = "time::serde::rfc3339")]
    bucket: OffsetDateTime,
    total: i64,
    accepted: i64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BalloonMetrics {
    total: i64,
    first_blood: i64,
    pending: i64,
    preparing: i64,
    delivering: i64,
    delivered: i64,
    cancelled: i64,
    colors: Vec<CountByName>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionMetrics {
    total: i64,
    accepted: i64,
    pending: i64,
    languages: Vec<CountByName>,
    trend: Vec<TrendPoint>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PresentationMetrics {
    balloons: BalloonMetrics,
    submissions: SubmissionMetrics,
}

fn require_live_operator(actor: &AuthUser) -> Result<(), AppError> {
    if actor.is_super_admin()
        || actor.has_permission(crate::features::auth::permissions::LIVE_MANAGE)
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "LIVE_PERMISSION_REQUIRED",
            "Live management permission is required",
        ))
    }
}
fn hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

async fn require_access(
    database: &PgPool,
    contest: i64,
    mode: &str,
    token: Option<&str>,
) -> Result<ConfigResponse, AppError> {
    let mode = match mode.trim().to_ascii_uppercase().as_str() {
        "SCREEN" => "SCREEN",
        "LIVE" => "LIVE",
        _ => return Err(AppError::validation("mode", "must be SCREEN or LIVE")),
    };
    require_contest(database, contest).await?;
    let public = sqlx::query_scalar::<_, bool>(
        "SELECT visibility = 'PUBLIC' FROM contests WHERE id=$1 AND deleted_at IS NULL",
    )
    .bind(contest)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check presentation visibility", error))?;
    if !public {
        return Err(AppError::not_found(
            "PRESENTATION_NOT_PUBLISHED",
            "Presentation is not published",
        ));
    }
    if mode == "LIVE" {
        let raw = token.filter(|value| !value.trim().is_empty()).ok_or_else(|| {
            AppError::unauthorized("BROADCAST_TOKEN_INVALID", "Broadcast token is invalid")
        })?;
        let changed = sqlx::query("UPDATE broadcast_tokens SET last_used_at=now() WHERE contest_id=$1 AND token_hash=$2 AND revoked_at IS NULL AND expires_at>now()")
            .bind(contest).bind(hash(raw)).execute(database).await.map_err(|e| AppError::internal("validate broadcast token", e))?.rows_affected();
        if changed != 1 {
            return Err(AppError::unauthorized(
                "BROADCAST_TOKEN_INVALID",
                "Broadcast token is invalid",
            ));
        }
    }
    sqlx::query_as("SELECT c.contest_id,c.mode,c.enabled,c.title,c.subtitle,c.accent_color,c.row_limit,c.show_announcements,c.announcement_interval_seconds,c.template,c.custom_template_id,t.name AS custom_template_name,t.background_color AS custom_background_color,t.foreground_color AS custom_foreground_color,t.accent_color AS custom_accent_color,t.font_family AS custom_font_family,t.density AS custom_density,t.show_clock AS custom_show_clock,t.show_logo AS custom_show_logo,t.logo_object_key AS custom_logo_object_key,c.updated_at FROM presentation_configs c LEFT JOIN presentation_templates t ON t.id=c.custom_template_id WHERE c.contest_id=$1 AND c.mode=$2 AND c.enabled")
        .bind(contest).bind(mode).fetch_optional(database).await.map_err(|e| AppError::internal("load published presentation", e))?.ok_or_else(|| AppError::not_found("PRESENTATION_NOT_PUBLISHED", "Presentation is not published"))
}

fn supplied_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-broadcast-token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
}

#[utoipa::path(get, path = "/api/public/presentations/{contest_id}", operation_id = "getPublishedPresentation", tag = "live", params(("contest_id" = i64, Path), ("mode" = String, Query)), responses((status = 200, body = PublishedPresentation), (status = 401, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security((), ("broadcast_token_header" = [])))]
pub async fn published(
    State(state): State<AppState>,
    Path(contest): Path<i64>,
    Query(query): Query<PublishedQuery>,
    headers: HeaderMap,
) -> Result<Json<PublishedPresentation>, AppError> {
    let config =
        require_access(state.database(), contest, &query.mode, supplied_token(&headers)).await?;
    let row = sqlx::query_as::<
        _,
        (String, String, Option<OffsetDateTime>, Option<OffsetDateTime>, Option<OffsetDateTime>),
    >("SELECT name,status,start_at,freeze_at,end_at FROM contests WHERE id=$1 AND deleted_at IS NULL")
    .bind(contest)
    .fetch_one(state.database())
    .await
    .map_err(|e| AppError::internal("load presentation contest", e))?;
    let scoreboard = state
        .scoreboard()
        .public(contest, ScoreboardQuery { group_name: None, participation_type: None }.validate()?)
        .await?;
    let announcements = if config.show_announcements {
        sqlx::query_as("SELECT id,title,body,pinned,published_at FROM announcements WHERE contest_id=$1 AND status='PUBLISHED' AND withdrawn_at IS NULL ORDER BY pinned DESC,published_at DESC,id DESC").bind(contest).fetch_all(state.database()).await.map_err(|e| AppError::internal("load presentation announcements", e))?
    } else {
        vec![]
    };
    Ok(Json(PublishedPresentation {
        contest_id: contest,
        contest_name: row.0,
        contest_status: row.1,
        start_at: row.2,
        freeze_at: row.3,
        end_at: row.4,
        server_time: OffsetDateTime::now_utc(),
        config,
        scoreboard,
        announcements,
    }))
}

#[utoipa::path(get, path = "/api/public/presentations/{contest_id}/metrics", operation_id = "getPresentationMetrics", tag = "live", params(("contest_id" = i64, Path), ("mode" = String, Query)), responses((status = 200, body = PresentationMetrics), (status = 401, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security((), ("broadcast_token_header" = [])))]
pub async fn metrics(
    State(state): State<AppState>,
    Path(contest): Path<i64>,
    Query(query): Query<PublishedQuery>,
    headers: HeaderMap,
) -> Result<Json<PresentationMetrics>, AppError> {
    require_access(state.database(), contest, &query.mode, supplied_token(&headers)).await?;
    Ok(Json(load_metrics(state.database(), contest).await?))
}

async fn load_metrics(database: &PgPool, contest: i64) -> Result<PresentationMetrics, AppError> {
    let balloon = sqlx::query_as::<_, (i64,i64,i64,i64,i64,i64,i64)>("SELECT count(*),count(*) FILTER(WHERE is_first_blood),count(*) FILTER(WHERE upper(status)='PENDING'),0::bigint,count(*) FILTER(WHERE upper(status)='CLAIMED'),count(*) FILTER(WHERE upper(status)='DELIVERED'),count(*) FILTER(WHERE upper(status)='CANCELLED') FROM balloon_tasks WHERE contest_id=$1").bind(contest).fetch_one(database).await.map_err(|e| AppError::internal("load balloon presentation metrics", e))?;
    let colors = sqlx::query_as("SELECT coalesce(color,'未设置') AS name,count(*) AS total FROM balloon_tasks WHERE contest_id=$1 AND upper(status)<>'CANCELLED' GROUP BY color ORDER BY total DESC,name").bind(contest).fetch_all(database).await.map_err(|e| AppError::internal("load balloon colors", e))?;
    let submission = sqlx::query_as::<_, (i64,i64,i64)>("SELECT count(*),count(*) FILTER(WHERE status IN('AC','ACCEPTED')),count(*) FILTER(WHERE status IN('PENDING','JUDGING')) FROM submissions WHERE contest_id=$1").bind(contest).fetch_one(database).await.map_err(|e| AppError::internal("load submission presentation metrics", e))?;
    let languages = sqlx::query_as("SELECT language AS name,count(*) AS total FROM submissions WHERE contest_id=$1 GROUP BY language ORDER BY total DESC,name").bind(contest).fetch_all(database).await.map_err(|e| AppError::internal("load submission languages", e))?;
    let trend = sqlx::query_as("SELECT date_trunc('hour',submitted_at) AS bucket,count(*) AS total,count(*) FILTER(WHERE status IN('AC','ACCEPTED')) AS accepted FROM submissions WHERE contest_id=$1 AND submitted_at>=now()-interval '24 hours' GROUP BY bucket ORDER BY bucket").bind(contest).fetch_all(database).await.map_err(|e| AppError::internal("load submission trend", e))?;
    Ok(PresentationMetrics {
        balloons: BalloonMetrics {
            total: balloon.0,
            first_blood: balloon.1,
            pending: balloon.2,
            preparing: balloon.3,
            delivering: balloon.4,
            delivered: balloon.5,
            cancelled: balloon.6,
            colors,
        },
        submissions: SubmissionMetrics {
            total: submission.0,
            accepted: submission.1,
            pending: submission.2,
            languages,
            trend,
        },
    })
}

#[utoipa::path(get, path = "/api/presentation-configs/{contest_id}/live/tokens", operation_id = "listBroadcastTokens", tag = "live", params(("contest_id" = i64, Path)), responses((status = 200, body = [BroadcastTokenResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_tokens(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest): Path<i64>,
) -> Result<Json<Vec<BroadcastTokenResponse>>, AppError> {
    context.require_password_ready()?;
    require_live_operator(context.user())?;
    require_contest(state.database(), contest).await?;
    Ok(Json(sqlx::query_as("SELECT id,label,expires_at,revoked_at,last_used_at,created_at FROM broadcast_tokens WHERE contest_id=$1 ORDER BY created_at DESC,id DESC").bind(contest).fetch_all(state.database()).await.map_err(|e| AppError::internal("list broadcast tokens", e))?))
}

#[utoipa::path(post, path = "/api/presentation-configs/{contest_id}/live/tokens", operation_id = "createBroadcastToken", tag = "live", params(("contest_id" = i64, Path)), request_body = BroadcastTokenRequest, responses((status = 201, body = BroadcastTokenCreated), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create_token(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest): Path<i64>,
    payload: Result<Json<BroadcastTokenRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<BroadcastTokenCreated>), AppError> {
    context.require_password_ready()?;
    require_live_operator(context.user())?;
    require_contest(state.database(), contest).await?;
    let Json(mut request) =
        payload.map_err(|_| AppError::validation("request", "invalid broadcast token"))?;
    request.label = request.label.trim().to_owned();
    if request.label.is_empty() || request.label.chars().count() > 120 {
        return Err(AppError::validation("label", "must contain 1 to 120 characters"));
    }
    if request.expires_at <= OffsetDateTime::now_utc() {
        return Err(AppError::validation("expiresAt", "must be in the future"));
    }
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| AppError::internal("generate broadcast token", e))?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let mut tx = state
        .database()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin broadcast token", e))?;
    let row=sqlx::query_as::<_,(i64,OffsetDateTime)>("INSERT INTO broadcast_tokens(contest_id,label,token_hash,expires_at,created_by_user_id) VALUES($1,$2,$3,$4,$5) RETURNING id,created_at").bind(contest).bind(&request.label).bind(hash(&token)).bind(request.expires_at).bind(context.user().id).fetch_one(&mut *tx).await.map_err(|e|AppError::internal("create broadcast token",e))?;
    audit(
        &mut tx,
        context.user().id,
        "BROADCAST_TOKEN_CREATED",
        "BROADCAST_TOKEN",
        row.0,
        peer.ip(),
    )
    .await?;
    tx.commit().await.map_err(|e| AppError::internal("commit broadcast token", e))?;
    Ok((
        StatusCode::CREATED,
        Json(BroadcastTokenCreated {
            id: row.0,
            label: request.label,
            token,
            expires_at: request.expires_at,
            created_at: row.1,
        }),
    ))
}

#[utoipa::path(delete, path = "/api/presentation-configs/{contest_id}/live/tokens/{token_id}", operation_id = "revokeBroadcastToken", tag = "live", params(("contest_id" = i64, Path), ("token_id" = i64, Path)), responses((status = 204), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn revoke_token(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((contest, id)): Path<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    context.require_password_ready()?;
    require_live_operator(context.user())?;
    let mut tx = state
        .database()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin broadcast token revoke", e))?;
    let changed=sqlx::query("UPDATE broadcast_tokens SET revoked_at=coalesce(revoked_at,now()) WHERE id=$1 AND contest_id=$2").bind(id).bind(contest).execute(&mut *tx).await.map_err(|e|AppError::internal("revoke broadcast token",e))?.rows_affected();
    if changed != 1 {
        return Err(AppError::not_found(
            "BROADCAST_TOKEN_NOT_FOUND",
            "Broadcast token was not found",
        ));
    }
    audit(&mut tx, context.user().id, "BROADCAST_TOKEN_REVOKED", "BROADCAST_TOKEN", id, peer.ip())
        .await?;
    tx.commit().await.map_err(|e| AppError::internal("commit broadcast token revoke", e))?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;

    #[test]
    fn broadcast_token_is_accepted_only_from_a_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-broadcast-token", "header-token".parse().expect("header"));
        assert_eq!(supplied_token(&headers), Some("header-token"));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn live_tokens_gate_published_data_and_metrics(pool: PgPool) {
        let user = sqlx::query_scalar::<_, i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('live-op','hash','Live Op','STAFF') RETURNING id").fetch_one(&pool).await.expect("operator");
        let contest = sqlx::query_scalar::<_, i64>("INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES('Live Contest','RUNNING','PUBLIC',now()-interval '1 hour',now()+interval '1 hour',now()+interval '2 hours') RETURNING id").fetch_one(&pool).await.expect("contest");
        sqlx::query("INSERT INTO presentation_configs(contest_id,mode,enabled,updated_by_user_id) VALUES($1,'LIVE',true,$2)").bind(contest).bind(user).execute(&pool).await.expect("config");
        let raw = "raw-token-visible-once";
        let token = sqlx::query_scalar::<_, i64>("INSERT INTO broadcast_tokens(contest_id,label,token_hash,expires_at,created_by_user_id) VALUES($1,'OBS',$2,now()+interval '1 hour',$3) RETURNING id").bind(contest).bind(hash(raw)).bind(user).fetch_one(&pool).await.expect("token");
        assert!(require_access(&pool, contest, "LIVE", None).await.is_err());
        assert!(require_access(&pool, contest, "LIVE", Some("wrong")).await.is_err());
        let config = require_access(&pool, contest, "live", Some(raw)).await.expect("valid access");
        assert_eq!(config.mode, "LIVE");
        assert!(require_access(&pool, contest, "LIVE", Some(raw)).await.is_ok());
        assert!(
            sqlx::query_scalar::<_, Option<OffsetDateTime>>(
                "SELECT last_used_at FROM broadcast_tokens WHERE id=$1"
            )
            .bind(token)
            .fetch_one(&pool)
            .await
            .expect("last used")
            .is_some()
        );
        let metrics = load_metrics(&pool, contest).await.expect("empty metrics");
        assert_eq!(metrics.balloons.total, 0);
        assert_eq!(metrics.submissions.total, 0);
        sqlx::query("UPDATE broadcast_tokens SET revoked_at=now() WHERE id=$1")
            .bind(token)
            .execute(&pool)
            .await
            .expect("revoke");
        assert!(require_access(&pool, contest, "LIVE", Some(raw)).await.is_err());
        let stored =
            sqlx::query_scalar::<_, String>("SELECT token_hash FROM broadcast_tokens WHERE id=$1")
                .bind(token)
                .fetch_one(&pool)
                .await
                .expect("hash");
        assert_ne!(stored, raw);
        assert_eq!(stored, hash(raw));
    }
}
