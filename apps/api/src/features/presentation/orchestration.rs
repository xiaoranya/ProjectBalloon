use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
};

use axum::{
    Json,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    error::AppError,
    features::auth::{AuthContext, model::AuthUser},
    state::AppState,
};

use super::{audit, require_contest, require_screen_operator, validate_view};

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaylistItemRequest {
    pub target_view: String,
    pub duration_seconds: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaylistRequest {
    pub name: String,
    pub loop_enabled: bool,
    pub items: Vec<PlaylistItemRequest>,
    pub expected_version: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistItemResponse {
    pub id: i64,
    pub target_view: String,
    pub duration_seconds: i32,
    pub display_order: i32,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistResponse {
    pub id: i64,
    pub contest_id: i64,
    pub name: String,
    pub loop_enabled: bool,
    pub version: i64,
    #[sqlx(skip)]
    pub items: Vec<PlaylistItemResponse>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupRequest {
    pub name: String,
    pub instance_ids: Vec<i64>,
    pub expected_version: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupControlRequest {
    pub action: String,
    pub playlist_id: Option<i64>,
    pub target_view: Option<String>,
    pub expected_version: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct GroupResponse {
    pub id: i64,
    pub contest_id: i64,
    pub name: String,
    #[sqlx(skip)]
    pub instance_ids: Vec<i64>,
    pub playlist_id: Option<i64>,
    pub playback_status: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub playback_started_at: Option<OffsetDateTime>,
    pub paused_elapsed_seconds: i64,
    pub locked_view: Option<String>,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupPlaybackResponse {
    pub group_id: i64,
    pub group_name: String,
    pub playlist_id: Option<i64>,
    pub loop_enabled: bool,
    pub status: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub started_at: Option<OffsetDateTime>,
    pub paused_elapsed_seconds: i64,
    pub locked_view: Option<String>,
    pub version: i64,
    pub items: Vec<PlaylistItemResponse>,
}

pub struct OrchestrationService {
    database: PgPool,
}
impl OrchestrationService {
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    async fn list_playlists(
        &self,
        contest: i64,
        actor: &AuthUser,
    ) -> Result<Vec<PlaylistResponse>, AppError> {
        require_screen_operator(actor)?;
        require_contest(&self.database, contest).await?;
        let rows = sqlx::query_as("SELECT id,contest_id,name,loop_enabled,version,created_at,updated_at FROM screen_playlists WHERE contest_id=$1 ORDER BY created_at,id")
            .bind(contest).fetch_all(&self.database).await.map_err(|e| AppError::internal("list screen playlists", e))?;
        hydrate_playlists(&self.database, rows).await
    }

    async fn create_playlist(
        &self,
        contest: i64,
        mut request: PlaylistRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<PlaylistResponse, AppError> {
        require_screen_operator(actor)?;
        require_contest(&self.database, contest).await?;
        validate_playlist(&mut request)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|e| AppError::internal("begin screen playlist", e))?;
        let taken = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM screen_playlists WHERE contest_id=$1 AND name=$2)",
        )
        .bind(contest)
        .bind(&request.name)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::internal("check screen playlist name", e))?;
        if taken {
            return Err(conflict(
                "SCREEN_PLAYLIST_NAME_TAKEN",
                "Screen playlist name is already used",
            ));
        }
        let id = sqlx::query_scalar::<_, i64>("INSERT INTO screen_playlists(contest_id,name,loop_enabled,created_by_user_id) VALUES($1,$2,$3,$4) RETURNING id")
            .bind(contest).bind(&request.name).bind(request.loop_enabled).bind(actor.id).fetch_one(&mut *tx).await.map_err(|e| AppError::internal("create screen playlist", e))?;
        replace_items(&mut tx, id, &request.items).await?;
        audit(&mut tx, actor.id, "SCREEN_PLAYLIST_CREATED", "SCREEN_PLAYLIST", id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit screen playlist", e))?;
        load_playlist(&self.database, id).await
    }

    async fn update_playlist(
        &self,
        id: i64,
        mut request: PlaylistRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<PlaylistResponse, AppError> {
        require_screen_operator(actor)?;
        validate_playlist(&mut request)?;
        let expected = require_version(request.expected_version)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|e| AppError::internal("begin screen playlist update", e))?;
        let current = sqlx::query_as::<_, (i64, i64)>(
            "SELECT contest_id,version FROM screen_playlists WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::internal("load screen playlist", e))?
        .ok_or_else(|| {
            AppError::not_found("SCREEN_PLAYLIST_NOT_FOUND", "Screen playlist was not found")
        })?;
        if current.1 != expected {
            return Err(conflict(
                "SCREEN_PLAYLIST_VERSION_CONFLICT",
                "Screen playlist was changed",
            ));
        }
        let in_use = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM screen_groups WHERE playlist_id=$1)",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::internal("check screen playlist usage", e))?;
        if in_use {
            return Err(conflict("SCREEN_PLAYLIST_IN_USE", "Screen playlist is in use"));
        }
        let taken = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM screen_playlists WHERE contest_id=$1 AND name=$2 AND id<>$3)").bind(current.0).bind(&request.name).bind(id).fetch_one(&mut *tx).await.map_err(|e| AppError::internal("check screen playlist name", e))?;
        if taken {
            return Err(conflict(
                "SCREEN_PLAYLIST_NAME_TAKEN",
                "Screen playlist name is already used",
            ));
        }
        sqlx::query("UPDATE screen_playlists SET name=$2,loop_enabled=$3,version=version+1,updated_at=now() WHERE id=$1").bind(id).bind(&request.name).bind(request.loop_enabled).execute(&mut *tx).await.map_err(|e| AppError::internal("update screen playlist", e))?;
        replace_items(&mut tx, id, &request.items).await?;
        audit(&mut tx, actor.id, "SCREEN_PLAYLIST_UPDATED", "SCREEN_PLAYLIST", id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit screen playlist update", e))?;
        load_playlist(&self.database, id).await
    }

    async fn delete_playlist(&self, id: i64, actor: &AuthUser, ip: IpAddr) -> Result<(), AppError> {
        require_screen_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|e| AppError::internal("begin screen playlist delete", e))?;
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM screen_playlists WHERE id=$1)",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::internal("load screen playlist", e))?;
        if !exists {
            return Err(AppError::not_found(
                "SCREEN_PLAYLIST_NOT_FOUND",
                "Screen playlist was not found",
            ));
        }
        let used = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM screen_groups WHERE playlist_id=$1)",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::internal("check screen playlist usage", e))?;
        if used {
            return Err(conflict("SCREEN_PLAYLIST_IN_USE", "Screen playlist is in use"));
        }
        sqlx::query("DELETE FROM screen_playlists WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal("delete screen playlist", e))?;
        audit(&mut tx, actor.id, "SCREEN_PLAYLIST_DELETED", "SCREEN_PLAYLIST", id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit screen playlist delete", e))?;
        Ok(())
    }

    async fn list_groups(
        &self,
        contest: i64,
        actor: &AuthUser,
    ) -> Result<Vec<GroupResponse>, AppError> {
        require_screen_operator(actor)?;
        require_contest(&self.database, contest).await?;
        let rows = sqlx::query_as("SELECT id,contest_id,name,playlist_id,playback_status,playback_started_at,paused_elapsed_seconds,locked_view,version,created_at,updated_at FROM screen_groups WHERE contest_id=$1 ORDER BY created_at,id")
            .bind(contest).fetch_all(&self.database).await.map_err(|e| AppError::internal("list screen groups", e))?;
        hydrate_groups(&self.database, rows).await
    }

    async fn create_group(
        &self,
        contest: i64,
        mut request: GroupRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<GroupResponse, AppError> {
        require_screen_operator(actor)?;
        require_contest(&self.database, contest).await?;
        validate_group(&mut request)?;
        let mut tx =
            self.database.begin().await.map_err(|e| AppError::internal("begin screen group", e))?;
        ensure_group_name(&mut tx, contest, &request.name, None).await?;
        let id = sqlx::query_scalar::<_, i64>("INSERT INTO screen_groups(contest_id,name,created_by_user_id) VALUES($1,$2,$3) RETURNING id").bind(contest).bind(&request.name).bind(actor.id).fetch_one(&mut *tx).await.map_err(|e| AppError::internal("create screen group", e))?;
        replace_members(&mut tx, id, contest, &request.instance_ids).await?;
        audit(&mut tx, actor.id, "SCREEN_GROUP_CREATED", "SCREEN_GROUP", id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit screen group", e))?;
        load_group(&self.database, id).await
    }

    async fn update_group(
        &self,
        id: i64,
        mut request: GroupRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<GroupResponse, AppError> {
        require_screen_operator(actor)?;
        validate_group(&mut request)?;
        let expected = require_version(request.expected_version)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|e| AppError::internal("begin screen group update", e))?;
        let current = lock_group(&mut tx, id).await?;
        if current.1 != expected {
            return Err(conflict("SCREEN_GROUP_VERSION_CONFLICT", "Screen group was changed"));
        }
        ensure_group_name(&mut tx, current.0, &request.name, Some(id)).await?;
        replace_members(&mut tx, id, current.0, &request.instance_ids).await?;
        sqlx::query(
            "UPDATE screen_groups SET name=$2,version=version+1,updated_at=now() WHERE id=$1",
        )
        .bind(id)
        .bind(&request.name)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("update screen group", e))?;
        audit(&mut tx, actor.id, "SCREEN_GROUP_UPDATED", "SCREEN_GROUP", id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit screen group update", e))?;
        load_group(&self.database, id).await
    }

    async fn delete_group(&self, id: i64, actor: &AuthUser, ip: IpAddr) -> Result<(), AppError> {
        require_screen_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|e| AppError::internal("begin screen group delete", e))?;
        lock_group(&mut tx, id).await?;
        sqlx::query("DELETE FROM screen_groups WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal("delete screen group", e))?;
        audit(&mut tx, actor.id, "SCREEN_GROUP_DELETED", "SCREEN_GROUP", id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit screen group delete", e))?;
        Ok(())
    }

    async fn control(
        &self,
        id: i64,
        request: GroupControlRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<GroupResponse, AppError> {
        require_screen_operator(actor)?;
        let expected = require_version(request.expected_version)?;
        let action = request.action.trim().to_ascii_uppercase();
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|e| AppError::internal("begin screen group control", e))?;
        let (contest, version) = lock_group(&mut tx, id).await?;
        if version != expected {
            return Err(conflict("SCREEN_GROUP_VERSION_CONFLICT", "Screen group was changed"));
        }
        let state = sqlx::query_as::<_, (String, Option<OffsetDateTime>, i64, Option<i64>)>("SELECT playback_status,playback_started_at,paused_elapsed_seconds,playlist_id FROM screen_groups WHERE id=$1").bind(id).fetch_one(&mut *tx).await.map_err(|e| AppError::internal("load screen group state", e))?;
        match action.as_str() {
            "PLAY" => {
                let playlist = request.playlist_id.ok_or_else(|| {
                    AppError::validation("playlistId", "SCREEN_PLAYLIST_REQUIRED")
                })?;
                let valid = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM screen_playlists p JOIN screen_playlist_items i ON i.playlist_id=p.id WHERE p.id=$1 AND p.contest_id=$2)").bind(playlist).bind(contest).fetch_one(&mut *tx).await.map_err(|e| AppError::internal("check screen playlist", e))?;
                if !valid {
                    return Err(AppError::not_found(
                        "SCREEN_PLAYLIST_NOT_FOUND",
                        "Screen playlist was not found or empty",
                    ));
                }
                sqlx::query("UPDATE screen_groups SET playlist_id=$2,playback_status='PLAYING',playback_started_at=now(),paused_elapsed_seconds=0,locked_view=NULL WHERE id=$1").bind(id).bind(playlist).execute(&mut *tx).await.map_err(|e| AppError::internal("play screen group", e))?;
            }
            "PAUSE" => {
                if state.0 != "PLAYING" || state.1.is_none() {
                    return Err(conflict(
                        "SCREEN_GROUP_NOT_PLAYING",
                        "Screen group is not playing",
                    ));
                }
                sqlx::query("UPDATE screen_groups SET playback_status='PAUSED',paused_elapsed_seconds=greatest(0,extract(epoch FROM (now()-playback_started_at))::bigint) WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(|e| AppError::internal("pause screen group", e))?;
            }
            "RESUME" => {
                if state.0 != "PAUSED" || state.3.is_none() {
                    return Err(conflict("SCREEN_GROUP_NOT_PAUSED", "Screen group is not paused"));
                }
                sqlx::query("UPDATE screen_groups SET playback_status='PLAYING',playback_started_at=now()-make_interval(secs => paused_elapsed_seconds::double precision) WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(|e| AppError::internal("resume screen group", e))?;
            }
            "STOP" => {
                sqlx::query("UPDATE screen_groups SET playback_status='STOPPED',playlist_id=NULL,playback_started_at=NULL,paused_elapsed_seconds=0 WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(|e| AppError::internal("stop screen group", e))?;
            }
            "LOCK" => {
                let target = request
                    .target_view
                    .as_deref()
                    .ok_or_else(|| AppError::validation("targetView", "SCREEN_TARGET_REQUIRED"))?;
                let target = validate_view(target)?;
                sqlx::query("UPDATE screen_groups SET locked_view=$2 WHERE id=$1")
                    .bind(id)
                    .bind(target)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::internal("lock screen group", e))?;
            }
            "UNLOCK" => {
                sqlx::query("UPDATE screen_groups SET locked_view=NULL WHERE id=$1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::internal("unlock screen group", e))?;
            }
            _ => {
                return Err(AppError::validation(
                    "action",
                    "must be PLAY, PAUSE, RESUME, STOP, LOCK or UNLOCK",
                ));
            }
        }
        sqlx::query("UPDATE screen_groups SET version=version+1,updated_at=now() WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal("version screen group", e))?;
        audit(&mut tx, actor.id, "SCREEN_GROUP_CONTROLLED", "SCREEN_GROUP", id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit screen group control", e))?;
        load_group(&self.database, id).await
    }
}

fn conflict(code: &'static str, message: &'static str) -> AppError {
    AppError::conflict(code, message)
}
fn require_version(value: Option<i64>) -> Result<i64, AppError> {
    value.ok_or_else(|| conflict("SCREEN_VERSION_REQUIRED", "Expected version is required"))
}
fn normalized_name(value: &str) -> String {
    value.trim().to_owned()
}
fn validate_playlist(request: &mut PlaylistRequest) -> Result<(), AppError> {
    request.name = normalized_name(&request.name);
    if request.name.is_empty() || request.name.chars().count() > 120 {
        return Err(AppError::validation("name", "must contain 1 to 120 characters"));
    }
    if request.items.is_empty() || request.items.len() > 20 {
        return Err(AppError::validation("items", "must contain 1 to 20 items"));
    }
    for item in &mut request.items {
        item.target_view = validate_view(&item.target_view)?.to_owned();
        if !(5..=3600).contains(&item.duration_seconds) {
            return Err(AppError::validation("durationSeconds", "must be between 5 and 3600"));
        }
    }
    Ok(())
}
fn validate_group(request: &mut GroupRequest) -> Result<(), AppError> {
    request.name = normalized_name(&request.name);
    if request.name.is_empty() || request.name.chars().count() > 120 {
        return Err(AppError::validation("name", "must contain 1 to 120 characters"));
    }
    let mut seen = HashSet::new();
    request.instance_ids.retain(|id| seen.insert(*id));
    if request.instance_ids.len() > 100 || request.instance_ids.iter().any(|id| *id <= 0) {
        return Err(AppError::validation(
            "instanceIds",
            "must contain up to 100 valid instance ids",
        ));
    }
    Ok(())
}
async fn replace_items(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
    items: &[PlaylistItemRequest],
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM screen_playlist_items WHERE playlist_id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::internal("replace screen playlist items", e))?;
    for (index, item) in items.iter().enumerate() {
        sqlx::query("INSERT INTO screen_playlist_items(playlist_id,target_view,duration_seconds,display_order) VALUES($1,$2,$3,$4)").bind(id).bind(&item.target_view).bind(item.duration_seconds).bind(i32::try_from(index + 1).unwrap_or(i32::MAX)).execute(&mut **tx).await.map_err(|e| AppError::internal("create screen playlist item", e))?;
    }
    Ok(())
}
async fn ensure_group_name(
    tx: &mut Transaction<'_, Postgres>,
    contest: i64,
    name: &str,
    excluded: Option<i64>,
) -> Result<(), AppError> {
    let taken = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM screen_groups WHERE contest_id=$1 AND name=$2 AND ($3::bigint IS NULL OR id<>$3))").bind(contest).bind(name).bind(excluded).fetch_one(&mut **tx).await.map_err(|e| AppError::internal("check screen group name", e))?;
    if taken {
        Err(conflict("SCREEN_GROUP_NAME_TAKEN", "Screen group name is already used"))
    } else {
        Ok(())
    }
}
async fn lock_group(tx: &mut Transaction<'_, Postgres>, id: i64) -> Result<(i64, i64), AppError> {
    sqlx::query_as("SELECT contest_id,version FROM screen_groups WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AppError::internal("load screen group", e))?
        .ok_or_else(|| AppError::not_found("SCREEN_GROUP_NOT_FOUND", "Screen group was not found"))
}
async fn replace_members(
    tx: &mut Transaction<'_, Postgres>,
    group: i64,
    contest: i64,
    ids: &[i64],
) -> Result<(), AppError> {
    let valid = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM screen_instances WHERE id=ANY($1) AND contest_id=$2 AND revoked_at IS NULL").bind(ids).bind(contest).fetch_one(&mut **tx).await.map_err(|e| AppError::internal("validate screen group instances", e))?;
    if usize::try_from(valid).unwrap_or(usize::MAX) != ids.len() {
        return Err(AppError::validation("instanceIds", "SCREEN_GROUP_INSTANCE_INVALID"));
    }
    let grouped = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM screen_group_members WHERE screen_instance_id=ANY($1) AND group_id<>$2)").bind(ids).bind(group).fetch_one(&mut **tx).await.map_err(|e| AppError::internal("check screen group membership", e))?;
    if grouped {
        return Err(conflict(
            "SCREEN_INSTANCE_ALREADY_GROUPED",
            "Screen instance already belongs to another group",
        ));
    }
    sqlx::query("DELETE FROM screen_group_members WHERE group_id=$1")
        .bind(group)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::internal("replace screen group members", e))?;
    for id in ids {
        sqlx::query("INSERT INTO screen_group_members(group_id,screen_instance_id) VALUES($1,$2)")
            .bind(group)
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::internal("add screen group member", e))?;
    }
    Ok(())
}
async fn hydrate_playlists(
    pool: &PgPool,
    mut rows: Vec<PlaylistResponse>,
) -> Result<Vec<PlaylistResponse>, AppError> {
    for row in &mut rows {
        row.items = sqlx::query_as("SELECT id,target_view,duration_seconds,display_order FROM screen_playlist_items WHERE playlist_id=$1 ORDER BY display_order").bind(row.id).fetch_all(pool).await.map_err(|e| AppError::internal("load screen playlist items", e))?;
    }
    Ok(rows)
}
async fn load_playlist(pool: &PgPool, id: i64) -> Result<PlaylistResponse, AppError> {
    let row = sqlx::query_as("SELECT id,contest_id,name,loop_enabled,version,created_at,updated_at FROM screen_playlists WHERE id=$1").bind(id).fetch_one(pool).await.map_err(|e| AppError::internal("load saved screen playlist", e))?;
    Ok(hydrate_playlists(pool, vec![row]).await?.remove(0))
}
async fn hydrate_groups(
    pool: &PgPool,
    mut rows: Vec<GroupResponse>,
) -> Result<Vec<GroupResponse>, AppError> {
    for row in &mut rows {
        row.instance_ids = sqlx::query_scalar("SELECT screen_instance_id FROM screen_group_members WHERE group_id=$1 ORDER BY created_at,id").bind(row.id).fetch_all(pool).await.map_err(|e| AppError::internal("load screen group members", e))?;
    }
    Ok(rows)
}
async fn load_group(pool: &PgPool, id: i64) -> Result<GroupResponse, AppError> {
    let row = sqlx::query_as("SELECT id,contest_id,name,playlist_id,playback_status,playback_started_at,paused_elapsed_seconds,locked_view,version,created_at,updated_at FROM screen_groups WHERE id=$1").bind(id).fetch_one(pool).await.map_err(|e| AppError::internal("load saved screen group", e))?;
    Ok(hydrate_groups(pool, vec![row]).await?.remove(0))
}

pub(super) async fn playback_for_instance(
    tx: &mut Transaction<'_, Postgres>,
    instance: i64,
) -> Result<Option<GroupPlaybackResponse>, AppError> {
    let row = sqlx::query_as::<_, (i64,String,Option<i64>,String,Option<OffsetDateTime>,i64,Option<String>,i64,Option<bool>)>("SELECT g.id,g.name,g.playlist_id,g.playback_status,g.playback_started_at,g.paused_elapsed_seconds,g.locked_view,g.version,p.loop_enabled FROM screen_group_members m JOIN screen_groups g ON g.id=m.group_id LEFT JOIN screen_playlists p ON p.id=g.playlist_id WHERE m.screen_instance_id=$1").bind(instance).fetch_optional(&mut **tx).await.map_err(|e| AppError::internal("load screen group playback", e))?;
    let Some(row) = row else { return Ok(None) };
    let items = if let Some(playlist) = row.2 {
        sqlx::query_as("SELECT id,target_view,duration_seconds,display_order FROM screen_playlist_items WHERE playlist_id=$1 ORDER BY display_order").bind(playlist).fetch_all(&mut **tx).await.map_err(|e| AppError::internal("load playback items", e))?
    } else {
        vec![]
    };
    Ok(Some(GroupPlaybackResponse {
        group_id: row.0,
        group_name: row.1,
        playlist_id: row.2,
        loop_enabled: row.8.unwrap_or(true),
        status: row.3,
        started_at: row.4,
        paused_elapsed_seconds: row.5,
        locked_view: row.6,
        version: row.7,
        items,
    }))
}

fn orchestration(state: &AppState) -> OrchestrationService {
    OrchestrationService::new(state.database().clone())
}
macro_rules! auth {
    ($context:expr) => {{
        $context.require_password_ready()?;
        require_screen_operator($context.user())?;
    }};
}
#[utoipa::path(get, path = "/api/contests/{contest_id}/screen-playlists", operation_id = "listScreenPlaylists", tag = "screens", params(("contest_id" = i64, Path)), responses((status = 200, body = [PlaylistResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_playlists(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest): Path<i64>,
) -> Result<Json<Vec<PlaylistResponse>>, AppError> {
    auth!(context);
    Ok(Json(orchestration(&state).list_playlists(contest, context.user()).await?))
}
#[utoipa::path(post, path = "/api/contests/{contest_id}/screen-playlists", operation_id = "createScreenPlaylist", tag = "screens", params(("contest_id" = i64, Path)), request_body = PlaylistRequest, responses((status = 201, body = PlaylistResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create_playlist(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest): Path<i64>,
    payload: Result<Json<PlaylistRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<PlaylistResponse>), AppError> {
    auth!(context);
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen playlist"))?;
    Ok((
        StatusCode::CREATED,
        Json(
            orchestration(&state)
                .create_playlist(contest, request, context.user(), peer.ip())
                .await?,
        ),
    ))
}
#[utoipa::path(put, path = "/api/screen-playlists/{playlist_id}", operation_id = "updateScreenPlaylist", tag = "screens", params(("playlist_id" = i64, Path)), request_body = PlaylistRequest, responses((status = 200, body = PlaylistResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_playlist(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<PlaylistRequest>, JsonRejection>,
) -> Result<Json<PlaylistResponse>, AppError> {
    auth!(context);
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen playlist"))?;
    Ok(Json(orchestration(&state).update_playlist(id, request, context.user(), peer.ip()).await?))
}
#[utoipa::path(delete, path = "/api/screen-playlists/{playlist_id}", operation_id = "deleteScreenPlaylist", tag = "screens", params(("playlist_id" = i64, Path)), responses((status = 204), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn delete_playlist(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    auth!(context);
    orchestration(&state).delete_playlist(id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(get, path = "/api/contests/{contest_id}/screen-groups", operation_id = "listScreenGroups", tag = "screens", params(("contest_id" = i64, Path)), responses((status = 200, body = [GroupResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_groups(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest): Path<i64>,
) -> Result<Json<Vec<GroupResponse>>, AppError> {
    auth!(context);
    Ok(Json(orchestration(&state).list_groups(contest, context.user()).await?))
}
#[utoipa::path(post, path = "/api/contests/{contest_id}/screen-groups", operation_id = "createScreenGroup", tag = "screens", params(("contest_id" = i64, Path)), request_body = GroupRequest, responses((status = 201, body = GroupResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create_group(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest): Path<i64>,
    payload: Result<Json<GroupRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<GroupResponse>), AppError> {
    auth!(context);
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen group"))?;
    Ok((
        StatusCode::CREATED,
        Json(
            orchestration(&state).create_group(contest, request, context.user(), peer.ip()).await?,
        ),
    ))
}
#[utoipa::path(put, path = "/api/screen-groups/{group_id}", operation_id = "updateScreenGroup", tag = "screens", params(("group_id" = i64, Path)), request_body = GroupRequest, responses((status = 200, body = GroupResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_group(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<GroupRequest>, JsonRejection>,
) -> Result<Json<GroupResponse>, AppError> {
    auth!(context);
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen group"))?;
    Ok(Json(orchestration(&state).update_group(id, request, context.user(), peer.ip()).await?))
}
#[utoipa::path(post, path = "/api/screen-groups/{group_id}/control", operation_id = "controlScreenGroup", tag = "screens", params(("group_id" = i64, Path)), request_body = GroupControlRequest, responses((status = 200, body = GroupResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn control_group(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<GroupControlRequest>, JsonRejection>,
) -> Result<Json<GroupResponse>, AppError> {
    auth!(context);
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen group control"))?;
    Ok(Json(orchestration(&state).control(id, request, context.user(), peer.ip()).await?))
}
#[utoipa::path(delete, path = "/api/screen-groups/{group_id}", operation_id = "deleteScreenGroup", tag = "screens", params(("group_id" = i64, Path)), responses((status = 204), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn delete_group(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    auth!(context);
    orchestration(&state).delete_group(id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use sqlx::PgPool;

    use super::*;
    use crate::features::{
        auth::model::{AuthUser, UserType},
        presentation::{ConfigRequest, HeartbeatRequest, PresentationService, RegisterRequest},
    };

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn playlists_groups_and_heartbeat_share_an_optimistic_timeline(pool: PgPool) {
        let user = sqlx::query_scalar::<_, i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('orchestration-op','hash','Orchestration Op','SCREEN_OPERATOR') RETURNING id")
            .fetch_one(&pool).await.expect("screen operator");
        let contest = sqlx::query_scalar::<_, i64>("INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES('Orchestration Contest','RUNNING','PUBLIC',now()-interval '1 hour',now()+interval '1 hour',now()+interval '2 hours') RETURNING id")
            .fetch_one(&pool).await.expect("contest");
        let actor = AuthUser {
            id: user,
            username: "orchestration-op".into(),
            display_name: "Orchestration Op".into(),
            user_type: UserType::ScreenOperator,
            roles: vec!["SCREEN_OPERATOR".into()],
            password_reset_required: false,
        };
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let presentation = PresentationService::new(pool.clone());
        presentation
            .update_config(
                contest,
                "SCREEN",
                ConfigRequest {
                    enabled: true,
                    title: None,
                    subtitle: None,
                    accent_color: "#22c55e".into(),
                    row_limit: 12,
                    show_announcements: true,
                    announcement_interval_seconds: 10,
                    template: None,
                },
                &actor,
                ip,
            )
            .await
            .expect("publish");
        let registration = presentation
            .register(RegisterRequest { contest_id: contest, name: "Hall A".into() }, ip)
            .await
            .expect("register");
        let service = OrchestrationService::new(pool.clone());
        let playlist = service
            .create_playlist(
                contest,
                PlaylistRequest {
                    name: " Ceremony ".into(),
                    loop_enabled: true,
                    items: vec![
                        PlaylistItemRequest {
                            target_view: "scoreboard".into(),
                            duration_seconds: 10,
                        },
                        PlaylistItemRequest { target_view: "AWARDS".into(), duration_seconds: 5 },
                    ],
                    expected_version: None,
                },
                &actor,
                ip,
            )
            .await
            .expect("playlist");
        assert_eq!(playlist.name, "Ceremony");
        assert_eq!(playlist.items.len(), 2);
        let group = service
            .create_group(
                contest,
                GroupRequest {
                    name: "Main Hall".into(),
                    instance_ids: vec![registration.instance_id, registration.instance_id],
                    expected_version: None,
                },
                &actor,
                ip,
            )
            .await
            .expect("group");
        assert_eq!(group.instance_ids, vec![registration.instance_id]);
        let playing = service
            .control(
                group.id,
                GroupControlRequest {
                    action: "PLAY".into(),
                    playlist_id: Some(playlist.id),
                    target_view: None,
                    expected_version: Some(group.version),
                },
                &actor,
                ip,
            )
            .await
            .expect("play");
        assert_eq!(playing.playback_status, "PLAYING");
        assert_eq!(playing.version, 1);
        assert!(
            service
                .update_playlist(
                    playlist.id,
                    PlaylistRequest {
                        name: playlist.name.clone(),
                        loop_enabled: false,
                        items: vec![PlaylistItemRequest {
                            target_view: "AWARDS".into(),
                            duration_seconds: 10
                        }],
                        expected_version: Some(playlist.version)
                    },
                    &actor,
                    ip
                )
                .await
                .is_err()
        );
        assert!(
            service
                .control(
                    group.id,
                    GroupControlRequest {
                        action: "STOP".into(),
                        playlist_id: None,
                        target_view: None,
                        expected_version: Some(0)
                    },
                    &actor,
                    ip
                )
                .await
                .is_err()
        );
        let heartbeat = presentation
            .heartbeat(
                registration.instance_id,
                HeartbeatRequest {
                    client_token: registration.client_token,
                    current_view: "SCOREBOARD".into(),
                },
                ip,
            )
            .await
            .expect("heartbeat");
        let playback = heartbeat.group_playback.expect("group playback");
        assert_eq!(playback.group_id, group.id);
        assert_eq!(playback.playlist_id, Some(playlist.id));
        assert_eq!(playback.items.len(), 2);
        let paused = service
            .control(
                group.id,
                GroupControlRequest {
                    action: "PAUSE".into(),
                    playlist_id: None,
                    target_view: None,
                    expected_version: Some(playing.version),
                },
                &actor,
                ip,
            )
            .await
            .expect("pause");
        assert_eq!(paused.playback_status, "PAUSED");
        let resumed = service
            .control(
                group.id,
                GroupControlRequest {
                    action: "RESUME".into(),
                    playlist_id: None,
                    target_view: None,
                    expected_version: Some(paused.version),
                },
                &actor,
                ip,
            )
            .await
            .expect("resume");
        assert_eq!(resumed.playback_status, "PLAYING");
        let stopped = service
            .control(
                group.id,
                GroupControlRequest {
                    action: "STOP".into(),
                    playlist_id: None,
                    target_view: None,
                    expected_version: Some(resumed.version),
                },
                &actor,
                ip,
            )
            .await
            .expect("stop");
        assert_eq!(stopped.playback_status, "STOPPED");
        assert!(stopped.playlist_id.is_none());
        service.delete_group(group.id, &actor, ip).await.expect("delete group");
        service.delete_playlist(playlist.id, &actor, ip).await.expect("delete playlist");
    }
}
