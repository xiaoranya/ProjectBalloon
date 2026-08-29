use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use time::OffsetDateTime;

use crate::{error::AppError, features::auth::model::AuthUser};

use crate::features::presentation::orchestration::model::{
    GroupControlRequest, GroupPlaybackResponse, GroupRequest, GroupResponse, PlaylistItemRequest,
    PlaylistItemResponse, PlaylistRequest, PlaylistResponse,
};
use crate::features::presentation::service::{
    audit, require_contest, require_screen_operator, validate_view,
};

pub struct OrchestrationService {
    database: PgPool,
}
impl OrchestrationService {
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub(super) async fn list_playlists(
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

    pub(super) async fn create_playlist(
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

    pub(super) async fn update_playlist(
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
            "SELECT playlist.contest_id,playlist.version FROM screen_playlists playlist JOIN contests contest ON contest.id = playlist.contest_id AND contest.deleted_at IS NULL WHERE playlist.id=$1 FOR UPDATE OF playlist",
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

    pub(super) async fn delete_playlist(
        &self,
        id: i64,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<(), AppError> {
        require_screen_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|e| AppError::internal("begin screen playlist delete", e))?;
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM screen_playlists playlist JOIN contests contest ON contest.id = playlist.contest_id AND contest.deleted_at IS NULL WHERE playlist.id=$1)",
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

    pub(super) async fn list_groups(
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

    pub(super) async fn create_group(
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
        let id = sqlx::query_scalar::<_, i64>("INSERT INTO screen_groups(contest_id,name,created_by_user_id) VALUES($1,$2,$3) RETURNING id").bind(contest).bind(&request.name).bind(actor.id).fetch_one(&mut *tx).await.map_err(|e| match e {
            sqlx::Error::Database(db) if db.constraint() == Some("screen_groups_contest_id_name_key") => conflict("SCREEN_GROUP_NAME_TAKEN", "Screen group name is already used"),
            other => AppError::internal("create screen group", other),
        })?;
        replace_members(&mut tx, id, contest, &request.instance_ids).await?;
        audit(&mut tx, actor.id, "SCREEN_GROUP_CREATED", "SCREEN_GROUP", id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit screen group", e))?;
        load_group(&self.database, id).await
    }

    pub(super) async fn update_group(
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

    pub(super) async fn delete_group(
        &self,
        id: i64,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<(), AppError> {
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

    pub(super) async fn control(
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
    let mut query = QueryBuilder::<Postgres>::new(
        "INSERT INTO screen_playlist_items(playlist_id,target_view,duration_seconds,display_order) ",
    );
    query.push_values(items.iter().enumerate(), |mut bind, (index, item)| {
        bind.push_bind(id)
            .push_bind(&item.target_view)
            .push_bind(item.duration_seconds)
            .push_bind(i32::try_from(index + 1).unwrap_or(i32::MAX));
    });
    query
        .build()
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::internal("create screen playlist items", e))?;
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
    sqlx::query_as("SELECT group_row.contest_id,group_row.version FROM screen_groups group_row JOIN contests contest ON contest.id = group_row.contest_id AND contest.deleted_at IS NULL WHERE group_row.id=$1 FOR UPDATE OF group_row")
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
    if !ids.is_empty() {
        let mut query = QueryBuilder::<Postgres>::new(
            "INSERT INTO screen_group_members(group_id,screen_instance_id) ",
        );
        query.push_values(ids.iter(), |mut bind, id| {
            bind.push_bind(group).push_bind(*id);
        });
        query
            .build()
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::internal("add screen group members", e))?;
    }
    Ok(())
}
async fn hydrate_playlists(
    pool: &PgPool,
    mut rows: Vec<PlaylistResponse>,
) -> Result<Vec<PlaylistResponse>, AppError> {
    let playlist_ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
    if playlist_ids.is_empty() {
        return Ok(rows);
    }
    let items = sqlx::query_as::<_, (i64, i64, String, i32, i32)>(
        "SELECT playlist_id,id,target_view,duration_seconds,display_order FROM screen_playlist_items WHERE playlist_id=ANY($1) ORDER BY playlist_id,display_order",
    )
    .bind(&playlist_ids)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::internal("load screen playlist items", e))?;
    let mut items_by_playlist = HashMap::<i64, Vec<PlaylistItemResponse>>::new();
    for (playlist_id, id, target_view, duration_seconds, display_order) in items {
        items_by_playlist.entry(playlist_id).or_default().push(PlaylistItemResponse {
            id,
            target_view,
            duration_seconds,
            display_order,
        });
    }
    for row in &mut rows {
        row.items = items_by_playlist.remove(&row.id).unwrap_or_default();
    }
    Ok(rows)
}
async fn load_playlist(pool: &PgPool, id: i64) -> Result<PlaylistResponse, AppError> {
    let row = sqlx::query_as(
        r#"
        SELECT playlist.id,playlist.contest_id,playlist.name,playlist.loop_enabled,
               playlist.version,playlist.created_at,playlist.updated_at
        FROM screen_playlists playlist
        JOIN contests contest
            ON contest.id = playlist.contest_id AND contest.deleted_at IS NULL
        WHERE playlist.id=$1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::internal("load saved screen playlist", e))?;
    Ok(hydrate_playlists(pool, vec![row]).await?.remove(0))
}
async fn hydrate_groups(
    pool: &PgPool,
    mut rows: Vec<GroupResponse>,
) -> Result<Vec<GroupResponse>, AppError> {
    let group_ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
    if group_ids.is_empty() {
        return Ok(rows);
    }
    let members = sqlx::query_as::<_, (i64, i64)>(
        "SELECT group_id,screen_instance_id FROM screen_group_members WHERE group_id=ANY($1) ORDER BY group_id,created_at,id",
    )
    .bind(&group_ids)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::internal("load screen group members", e))?;
    let mut members_by_group = HashMap::<i64, Vec<i64>>::new();
    for (group_id, instance_id) in members {
        members_by_group.entry(group_id).or_default().push(instance_id);
    }
    for row in &mut rows {
        row.instance_ids = members_by_group.remove(&row.id).unwrap_or_default();
    }
    Ok(rows)
}
async fn load_group(pool: &PgPool, id: i64) -> Result<GroupResponse, AppError> {
    let row = sqlx::query_as(
        r#"
        SELECT group_row.id,group_row.contest_id,group_row.name,group_row.playlist_id,
               group_row.playback_status,group_row.playback_started_at,
               group_row.paused_elapsed_seconds,group_row.locked_view,group_row.version,
               group_row.created_at,group_row.updated_at
        FROM screen_groups group_row
        JOIN contests contest
            ON contest.id = group_row.contest_id AND contest.deleted_at IS NULL
        WHERE group_row.id=$1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::internal("load saved screen group", e))?;
    Ok(hydrate_groups(pool, vec![row]).await?.remove(0))
}

pub(crate) async fn playback_for_instance(
    tx: &mut Transaction<'_, Postgres>,
    instance: i64,
) -> Result<Option<GroupPlaybackResponse>, AppError> {
    let row = sqlx::query_as::<
        _,
        (
            i64,
            String,
            Option<i64>,
            String,
            Option<OffsetDateTime>,
            i64,
            Option<String>,
            i64,
            Option<bool>,
        ),
    >(
        r#"
        SELECT g.id,g.name,g.playlist_id,g.playback_status,g.playback_started_at,
               g.paused_elapsed_seconds,g.locked_view,g.version,p.loop_enabled
        FROM screen_group_members m
        JOIN screen_groups g
            ON g.id=m.group_id
        JOIN contests contest
            ON contest.id = g.contest_id AND contest.deleted_at IS NULL
        LEFT JOIN screen_playlists p
            ON p.id=g.playlist_id
        WHERE m.screen_instance_id=$1
        "#,
    )
    .bind(instance)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::internal("load screen group playback", e))?;
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
