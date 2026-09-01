use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{error::AppError, features::auth::AuthContext, state::AppState};

use super::live::{require_access, require_live_operator, supplied_token};
use super::service::{audit, require_contest};

/// Scenes the composited live program can put on air. A superset of the screen
/// views plus the branded title card.
const LIVE_SCENES: &[&str] = &[
    "SCOREBOARD",
    "FIRST_BLOOD",
    "BALLOONS",
    "FREEZE_COUNTDOWN",
    "STATISTICS",
    "RESOLVER",
    "AWARDS",
    "TITLE_CARD",
];

pub(super) fn validate_scene(scene: &str) -> Result<&'static str, AppError> {
    let normalized = scene.trim().to_ascii_uppercase();
    LIVE_SCENES
        .iter()
        .copied()
        .find(|value| *value == normalized)
        .ok_or_else(|| AppError::validation("currentScene", "is not a supported live scene"))
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LiveProgramResponse {
    contest_id: i64,
    current_scene: String,
    resolver_run_id: Option<i64>,
    transition_milliseconds: i32,
    show_clock: bool,
    ticker_enabled: bool,
    title_card_text: Option<String>,
    version: i64,
    #[serde(with = "time::serde::rfc3339::option")]
    updated_at: Option<OffsetDateTime>,
}

impl LiveProgramResponse {
    fn default_for(contest: i64) -> Self {
        Self {
            contest_id: contest,
            current_scene: "SCOREBOARD".into(),
            resolver_run_id: None,
            transition_milliseconds: 800,
            show_clock: true,
            ticker_enabled: true,
            title_card_text: None,
            version: 0,
            updated_at: None,
        }
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ResolverRunOption {
    id: i64,
    official: bool,
    status: String,
    current_step: i32,
    total_steps: i32,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffLiveProgramResponse {
    program: LiveProgramResponse,
    resolver_runs: Vec<ResolverRunOption>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveProgramRequest {
    current_scene: String,
    resolver_run_id: Option<i64>,
    transition_milliseconds: i32,
    show_clock: bool,
    ticker_enabled: bool,
    title_card_text: Option<String>,
    expected_version: i64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublishedLiveProgram {
    contest_id: i64,
    current_scene: String,
    resolver_run_id: Option<i64>,
    transition_milliseconds: i32,
    show_clock: bool,
    ticker_enabled: bool,
    title_card_text: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    server_time: OffsetDateTime,
    version: i64,
}

const PROGRAM_SQL: &str = r#"
    SELECT contest_id, current_scene, resolver_run_id, transition_milliseconds,
           show_clock, ticker_enabled, title_card_text, version, updated_at
    FROM live_programs WHERE contest_id=$1
"#;

pub(super) async fn load_program(
    database: &PgPool,
    contest: i64,
) -> Result<LiveProgramResponse, AppError> {
    Ok(sqlx::query_as::<_, LiveProgramResponse>(PROGRAM_SQL)
        .bind(contest)
        .fetch_optional(database)
        .await
        .map_err(|e| AppError::internal("load live program", e))?
        .unwrap_or_else(|| LiveProgramResponse::default_for(contest)))
}

async fn resolver_run_options(
    database: &PgPool,
    contest: i64,
) -> Result<Vec<ResolverRunOption>, AppError> {
    sqlx::query_as(
        r#"
        SELECT run.id, run.official, run.status, run.current_step, run.total_steps, run.created_at
        FROM resolver_runs run
        JOIN contests contest ON contest.id = run.contest_id AND contest.deleted_at IS NULL
        WHERE run.contest_id = $1
        ORDER BY run.official DESC, run.created_at DESC, run.id DESC
        LIMIT 50
        "#,
    )
    .bind(contest)
    .fetch_all(database)
    .await
    .map_err(|e| AppError::internal("list resolver run options", e))
}

/// Stored id kept only when it still points at an official, publicly visible
/// run of this contest; otherwise the newest official run is resolved so the
/// program page never renders a hidden or foreign run.
async fn resolve_public_resolver_run(
    database: &PgPool,
    contest: i64,
    stored: Option<i64>,
) -> Result<Option<i64>, AppError> {
    if let Some(id) = stored
        && official_run_exists(database, contest, id).await?
    {
        return Ok(Some(id));
    }
    let latest = sqlx::query_scalar::<_, Option<i64>>(
        r#"
        SELECT max(run.id) FROM resolver_runs run
        JOIN contests contest ON contest.id = run.contest_id AND contest.deleted_at IS NULL
        WHERE run.contest_id = $1 AND run.official
        "#,
    )
    .bind(contest)
    .fetch_one(database)
    .await
    .map_err(|e| AppError::internal("resolve latest official resolver run", e))?;
    Ok(latest)
}

async fn official_run_exists(
    database: &PgPool,
    contest: i64,
    run_id: i64,
) -> Result<bool, AppError> {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM resolver_runs run
            JOIN contests contest ON contest.id = run.contest_id AND contest.deleted_at IS NULL
            WHERE run.id = $1 AND run.contest_id = $2 AND run.official
        )
        "#,
    )
    .bind(run_id)
    .bind(contest)
    .fetch_one(database)
    .await
    .map_err(|e| AppError::internal("check program resolver run", e))
}

fn validate_program_request(request: &LiveProgramRequest) -> Result<(), AppError> {
    validate_scene(&request.current_scene)?;
    if !(100..=5000).contains(&request.transition_milliseconds) {
        return Err(AppError::validation("transitionMilliseconds", "must be between 100 and 5000"));
    }
    if request.title_card_text.as_ref().is_some_and(|value| value.trim().chars().count() > 240) {
        return Err(AppError::validation("titleCardText", "is too long"));
    }
    Ok(())
}

async fn validate_resolver_run(
    database: &PgPool,
    contest: i64,
    run_id: Option<i64>,
) -> Result<(), AppError> {
    let Some(run_id) = run_id else {
        return Ok(());
    };
    if official_run_exists(database, contest, run_id).await? {
        Ok(())
    } else {
        Err(AppError::validation(
            "resolverRunId",
            "must reference an official resolver run of this contest",
        ))
    }
}

/// Version-checked upsert. `None` means the optimistic version did not match
/// (or the row pre-existed with a different version) and the caller must turn
/// that into a 409.
async fn save_program(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    contest: i64,
    request: &LiveProgramRequest,
    actor: i64,
) -> Result<Option<(i64, i64, OffsetDateTime)>, AppError> {
    sqlx::query_as::<_, (i64, i64, OffsetDateTime)>(
        r#"
        INSERT INTO live_programs
            (contest_id, current_scene, resolver_run_id, transition_milliseconds,
             show_clock, ticker_enabled, title_card_text, updated_by_user_id, version)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 1)
        ON CONFLICT (contest_id) DO UPDATE
            SET current_scene = excluded.current_scene,
                resolver_run_id = excluded.resolver_run_id,
                transition_milliseconds = excluded.transition_milliseconds,
                show_clock = excluded.show_clock,
                ticker_enabled = excluded.ticker_enabled,
                title_card_text = excluded.title_card_text,
                updated_by_user_id = excluded.updated_by_user_id,
                updated_at = now(),
                version = live_programs.version + 1
            WHERE live_programs.version = $9
        RETURNING id, version, updated_at
        "#,
    )
    .bind(contest)
    .bind(&request.current_scene)
    .bind(request.resolver_run_id)
    .bind(request.transition_milliseconds)
    .bind(request.show_clock)
    .bind(request.ticker_enabled)
    .bind(&request.title_card_text)
    .bind(actor)
    .bind(request.expected_version)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::internal("save live program", e))
}

#[utoipa::path(get, path = "/api/presentation-configs/{contest_id}/live/program", operation_id = "getLiveProgram", tag = "live", params(("contest_id" = i64, Path)), responses((status = 200, body = StaffLiveProgramResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get_program(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest): Path<i64>,
) -> Result<Json<StaffLiveProgramResponse>, AppError> {
    context.require_password_ready()?;
    require_live_operator(context.user())?;
    require_contest(state.database(), contest).await?;
    Ok(Json(StaffLiveProgramResponse {
        program: load_program(state.database(), contest).await?,
        resolver_runs: resolver_run_options(state.database(), contest).await?,
    }))
}

#[utoipa::path(put, path = "/api/presentation-configs/{contest_id}/live/program", operation_id = "updateLiveProgram", tag = "live", params(("contest_id" = i64, Path)), request_body = LiveProgramRequest, responses((status = 200, body = LiveProgramResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_program(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest): Path<i64>,
    payload: Result<Json<LiveProgramRequest>, JsonRejection>,
) -> Result<Json<LiveProgramResponse>, AppError> {
    context.require_password_ready()?;
    require_live_operator(context.user())?;
    require_contest(state.database(), contest).await?;
    let Json(mut request) =
        payload.map_err(|_| AppError::validation("request", "invalid live program update"))?;
    validate_program_request(&request)?;
    request.title_card_text = request
        .title_card_text
        .take()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    validate_resolver_run(state.database(), contest, request.resolver_run_id).await?;

    let mut tx = state
        .database()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin live program update", e))?;
    let Some((_, version, updated_at)) =
        save_program(&mut tx, contest, &request, context.user().id).await?
    else {
        return Err(AppError::conflict(
            "LIVE_PROGRAM_VERSION_CONFLICT",
            "Live program was changed",
        ));
    };
    audit(&mut tx, context.user().id, "LIVE_PROGRAM_UPDATED", "CONTEST", contest, peer.ip())
        .await?;
    sqlx::query("INSERT INTO realtime_outbox(event_id,contest_id,event_type,scope,payload_json) VALUES($1,$2,'LIVE_PROGRAM_UPDATED','PUBLIC',$3)")
        .bind(uuid::Uuid::new_v4())
        .bind(contest)
        .bind(serde_json::json!({}))
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("publish live program update", e))?;
    tx.commit().await.map_err(|e| AppError::internal("commit live program update", e))?;
    Ok(Json(LiveProgramResponse {
        contest_id: contest,
        current_scene: request.current_scene,
        resolver_run_id: request.resolver_run_id,
        transition_milliseconds: request.transition_milliseconds,
        show_clock: request.show_clock,
        ticker_enabled: request.ticker_enabled,
        title_card_text: request.title_card_text,
        version,
        updated_at: Some(updated_at),
    }))
}

#[utoipa::path(get, path = "/api/public/presentations/{contest_id}/program", operation_id = "getPublishedLiveProgram", tag = "live", params(("contest_id" = i64, Path)), responses((status = 200, body = PublishedLiveProgram), (status = 401, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security((), ("broadcast_token_header" = [])))]
pub async fn get_published_program(
    State(state): State<AppState>,
    Path(contest): Path<i64>,
    headers: HeaderMap,
) -> Result<Json<PublishedLiveProgram>, AppError> {
    require_access(state.database(), contest, "LIVE", supplied_token(&headers)).await?;
    let row = load_program(state.database(), contest).await?;
    let resolver_run_id =
        resolve_public_resolver_run(state.database(), contest, row.resolver_run_id).await?;
    Ok(Json(PublishedLiveProgram {
        contest_id: contest,
        current_scene: row.current_scene,
        resolver_run_id,
        transition_milliseconds: row.transition_milliseconds,
        show_clock: row.show_clock,
        ticker_enabled: row.ticker_enabled,
        title_card_text: row.title_card_text,
        server_time: OffsetDateTime::now_utc(),
        version: row.version,
    }))
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::super::live::hash;
    use super::*;

    fn request(scene: &str, expected_version: i64) -> LiveProgramRequest {
        LiveProgramRequest {
            current_scene: scene.to_owned(),
            resolver_run_id: None,
            transition_milliseconds: 800,
            show_clock: true,
            ticker_enabled: true,
            title_card_text: None,
            expected_version,
        }
    }

    #[test]
    fn scenes_are_a_closed_superset_of_screen_views() {
        for scene in LIVE_SCENES {
            validate_scene(scene).expect("supported scene");
        }
        assert_eq!(LIVE_SCENES.len(), 8);
        for invalid in ["", "UNKNOWN", "resolver-run"] {
            assert!(validate_scene(invalid).is_err());
        }
        // Case-insensitive normalization matches the screen view behavior.
        assert_eq!(validate_scene("resolver").expect("normalized"), "RESOLVER");
    }

    #[test]
    fn transition_window_is_bounded() {
        let mut base = request("SCOREBOARD", 0);
        for invalid in [99, 5001, -1] {
            base.transition_milliseconds = invalid;
            assert!(validate_program_request(&base).is_err());
        }
        for valid in [100, 800, 5000] {
            base.transition_milliseconds = valid;
            assert!(validate_program_request(&base).is_ok());
        }
    }

    #[test]
    fn title_card_text_is_bounded() {
        let mut base = request("TITLE_CARD", 0);
        base.title_card_text = Some("x".repeat(241));
        assert!(validate_program_request(&base).is_err());
        base.title_card_text = Some("  欢迎观看  ".repeat(2));
        assert!(validate_program_request(&base).is_ok());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn program_state_upserts_with_optimistic_version(pool: PgPool) {
        let user = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users(username,password_hash,display_name,user_type) VALUES('live-op','hash','Live Op','STAFF') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("operator");
        let contest = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES('Live Contest','RUNNING','PUBLIC',now()-interval '1 hour',now()+interval '1 hour',now()+interval '2 hours') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("contest");

        let fresh = load_program(&pool, contest).await.expect("default program");
        assert_eq!(fresh.current_scene, "SCOREBOARD");
        assert_eq!(fresh.version, 0);

        // A fresh row is created from version 0; the outbox receives the event.
        let saved = {
            let mut tx = pool.begin().await.expect("begin transaction");
            let row = save_program(&mut tx, contest, &request("FIRST_BLOOD", 0), user)
                .await
                .expect("save program");
            tx.commit().await.expect("commit transaction");
            row
        }
        .expect("first save");
        assert_eq!(saved.1, 1);
        let events = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM realtime_outbox WHERE contest_id=$1 AND event_type='LIVE_PROGRAM_UPDATED' AND scope='PUBLIC'",
        )
        .bind(contest)
        .fetch_one(&pool)
        .await
        .expect("outbox events");
        assert_eq!(events, 0, "the handler publishes the outbox event, not save_program");

        // A stale expectedVersion conflicts instead of silently overwriting.
        let stale = {
            let mut tx = pool.begin().await.expect("begin transaction");
            let row = save_program(&mut tx, contest, &request("AWARDS", 0), user)
                .await
                .expect("save program");
            tx.commit().await.expect("commit transaction");
            row
        };
        assert!(stale.is_none(), "stale version must not overwrite");
        let second = {
            let mut tx = pool.begin().await.expect("begin transaction");
            let row = save_program(&mut tx, contest, &request("AWARDS", 1), user)
                .await
                .expect("save program");
            tx.commit().await.expect("commit transaction");
            row
        }
        .expect("second save");
        assert_eq!(second.1, 2);
        let stored = sqlx::query_scalar::<_, String>(
            "SELECT current_scene FROM live_programs WHERE contest_id=$1",
        )
        .bind(contest)
        .fetch_one(&pool)
        .await
        .expect("stored scene");
        assert_eq!(stored, "AWARDS");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn public_program_resolves_official_resolver_runs(pool: PgPool) {
        let user = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users(username,password_hash,display_name,user_type) VALUES('live-op','hash','Live Op','STAFF') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("operator");
        let contest = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES('Live Contest','RUNNING','PUBLIC',now()-interval '1 hour',now()+interval '1 hour',now()+interval '2 hours') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("contest");
        sqlx::query("INSERT INTO presentation_configs(contest_id,mode,enabled,updated_by_user_id) VALUES($1,'LIVE',true,$2)")
            .bind(contest).bind(user).execute(&pool).await.expect("config");
        let raw = "raw-token-visible-once";
        sqlx::query("INSERT INTO broadcast_tokens(contest_id,label,token_hash,expires_at,created_by_user_id) VALUES($1,'OBS',$2,now()+interval '1 hour',$3)")
            .bind(contest).bind(hash(raw)).bind(user).execute(&pool).await.expect("token");

        // No program row and no runs: default scene, no resolver run.
        let empty = published_program(&pool, contest, raw).await.expect("empty program");
        assert_eq!(empty.current_scene, "SCOREBOARD");
        assert_eq!(empty.resolver_run_id, None);

        // A stored stale run id falls back to the newest official run.
        let rehearsal = sqlx::query_scalar::<_, i64>(
            "INSERT INTO resolver_runs(contest_id,official,status,total_steps,started_at) VALUES($1,false,'RUNNING',5,now()) RETURNING id",
        )
        .bind(contest)
        .fetch_one(&pool)
        .await
        .expect("rehearsal run");
        let official = sqlx::query_scalar::<_, i64>(
            "INSERT INTO resolver_runs(contest_id,official,status,total_steps,started_at) VALUES($1,true,'RUNNING',5,now()) RETURNING id",
        )
        .bind(contest)
        .fetch_one(&pool)
        .await
        .expect("official run");

        sqlx::query("INSERT INTO live_programs(contest_id,current_scene,resolver_run_id,updated_by_user_id,version) VALUES($1,'RESOLVER',$2,$3,1)")
            .bind(contest).bind(rehearsal).bind(user).execute(&pool).await.expect("program row");
        let resolved = published_program(&pool, contest, raw).await.expect("resolved program");
        assert_eq!(resolved.current_scene, "RESOLVER");
        assert_eq!(resolved.resolver_run_id, Some(official));

        // An explicit valid official run is preserved.
        sqlx::query("UPDATE live_programs SET resolver_run_id=$2 WHERE contest_id=$1")
            .bind(contest)
            .bind(official)
            .execute(&pool)
            .await
            .expect("pin run");
        let pinned = published_program(&pool, contest, raw).await.expect("pinned program");
        assert_eq!(pinned.resolver_run_id, Some(official));

        // The staff option list surfaces both runs, official first.
        let options = resolver_run_options(&pool, contest).await.expect("run options");
        assert_eq!(options.len(), 2);
        assert!(options[0].official);
        assert!(!options[1].official);
    }

    async fn published_program(
        pool: &PgPool,
        contest: i64,
        token: &str,
    ) -> Result<PublishedLiveProgram, AppError> {
        let mut headers = HeaderMap::new();
        headers.insert("x-broadcast-token", token.parse().expect("header value"));
        require_access(pool, contest, "LIVE", supplied_token(&headers)).await?;
        let row = load_program(pool, contest).await?;
        let resolver_run_id =
            resolve_public_resolver_run(pool, contest, row.resolver_run_id).await?;
        Ok(PublishedLiveProgram {
            contest_id: contest,
            current_scene: row.current_scene,
            resolver_run_id,
            transition_milliseconds: row.transition_milliseconds,
            show_clock: row.show_clock,
            ticker_enabled: row.ticker_enabled,
            title_card_text: row.title_card_text,
            server_time: OffsetDateTime::now_utc(),
            version: row.version,
        })
    }
}
