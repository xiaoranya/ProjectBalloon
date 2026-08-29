use std::net::IpAddr;

use uuid::Uuid;

use crate::error::AppError;
use crate::features::auth::model::AuthUser;

use crate::features::teams::model::{
    BatchImportResponse, BatchImportRowResponse, ParticipationType, ValidatedBatchImport,
};
use crate::features::teams::service::TeamService;
use crate::features::teams::service::contest_roster::{
    lock_open_contest, map_contest_team_write_error, require_manage_contest,
};
use crate::features::teams::service::helpers::{
    create_team_in_transaction, enqueue_realtime, prepare_team, record_audit,
};

impl TeamService {
    pub async fn batch_import(
        &self,
        request: ValidatedBatchImport,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<BatchImportResponse, AppError> {
        if !actor.is_super_admin()
            && actor.has_permission(crate::features::auth::permissions::CONTEST_MANAGE)
            && request.contest_id.is_none()
        {
            return Err(AppError::bad_request(
                "CONTEST_REQUIRED",
                "Contest managers must import into a managed contest",
            ));
        }
        let mut prepared = Vec::with_capacity(request.teams.len());
        for team in request.teams {
            prepared.push(prepare_team(team).await?);
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin team batch import", error))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&request.idempotency_key)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("lock team import idempotency key", error))?;
        if let Some((request_data, response_data)) =
            sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT request_data, response_data FROM team_import_batches WHERE idempotency_key = $1",
            )
            .bind(&request.idempotency_key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("load existing team import", error))?
        {
            if request_data != request.request_hash {
                return Err(AppError::conflict(
                    "TEAM_IMPORT_IDEMPOTENCY_CONFLICT",
                    "Idempotency key was already used for another request",
                ));
            }
            let response_data = response_data.ok_or_else(|| {
                AppError::conflict("TEAM_IMPORT_IN_PROGRESS", "Team import is still in progress")
            })?;
            return serde_json::from_str(&response_data)
                .map_err(|error| AppError::internal("deserialize team import response", error));
        }
        if let Some(contest_id) = request.contest_id {
            require_manage_contest(&mut transaction, contest_id, actor).await?;
            lock_open_contest(&mut transaction, contest_id).await?;
        } else if !actor.is_super_admin() {
            return Err(AppError::forbidden("FORBIDDEN", "Insufficient permissions"));
        }
        let batch_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO team_import_batches
                (batch_id, idempotency_key, request_data, created_by_user_id)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(&batch_id)
        .bind(&request.idempotency_key)
        .bind(&request.request_hash)
        .bind(actor.id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("create team import batch", error))?;
        let mut created = Vec::with_capacity(prepared.len());
        for (index, prepared_team) in prepared.into_iter().enumerate() {
            let star = prepared_team.request.star;
            let (team_id, account) =
                create_team_in_transaction(&mut transaction, prepared_team).await?;
            if let Some(contest_id) = request.contest_id {
                let participation =
                    if star { ParticipationType::Star } else { request.participation_type };
                sqlx::query(
                    r#"
                    INSERT INTO contest_teams
                        (contest_id, team_id, participation_type, group_name)
                    SELECT $1, $2, $3, group_name FROM teams WHERE id = $2
                    "#,
                )
                .bind(contest_id)
                .bind(team_id)
                .bind(participation.as_str())
                .execute(&mut *transaction)
                .await
                .map_err(map_contest_team_write_error)?;
                enqueue_realtime(
                    &mut transaction,
                    contest_id,
                    "CONTEST_TEAMS_CHANGED",
                    "TEAM",
                    Some(team_id),
                    serde_json::json!({
                        "contestId": contest_id,
                        "batchId": batch_id,
                        "action": "CONTEST_TEAM_ASSIGNED"
                    }),
                )
                .await?;
            }
            created.push(BatchImportRowResponse {
                index,
                team_id,
                user_id: account.as_ref().map(|(id, _)| *id),
                username: account.map(|(_, username)| username),
            });
        }
        let response = BatchImportResponse {
            batch_id: batch_id.clone(),
            total_requested: created.len(),
            created,
        };
        let response_json = serde_json::to_string(&response)
            .map_err(|error| AppError::internal("serialize team import response", error))?;
        sqlx::query(
            r#"
            UPDATE team_import_batches
            SET response_data = $1, completed_at = now()
            WHERE batch_id = $2
            "#,
        )
        .bind(response_json)
        .bind(&batch_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("complete team import batch", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "TEAM_BATCH_IMPORTED",
            "team_import_batch",
            &batch_id,
            request_ip,
            &format!("created:{}", response.total_requested),
        )
        .await?;
        if let Some(contest_id) = request.contest_id {
            enqueue_realtime(
                &mut transaction,
                contest_id,
                "CONTEST_TEAMS_CHANGED",
                "STAFF",
                None,
                serde_json::json!({"contestId": contest_id, "batchId": batch_id}),
            )
            .await?;
        }
        transaction.commit().await.map_err(map_batch_import_error)?;
        Ok(response)
    }
}

fn map_batch_import_error(error: sqlx::Error) -> AppError {
    match error.as_database_error().and_then(sqlx::error::DatabaseError::constraint) {
        Some("idx_teams_active_name_unique") => AppError::conflict(
            "TEAM_IMPORT_DUPLICATE_NAME",
            "Team import contains a duplicate name",
        ),
        Some("users_username_key") => AppError::conflict(
            "TEAM_IMPORT_DUPLICATE_USERNAME",
            "Team import contains a duplicate username",
        ),
        _ => AppError::internal("commit team batch import", error),
    }
}
