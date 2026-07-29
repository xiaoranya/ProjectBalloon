use std::net::{IpAddr, SocketAddr};

use axum::{
    Json,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    error::AppError,
    features::{
        auth::{AuthContext, model::AuthUser},
        scoreboard::{ScoreboardResponse, ScoreboardRow},
    },
    state::AppState,
};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CategoryRequest {
    code: String,
    name: String,
    display_order: i32,
    #[serde(default)]
    include_star: bool,
    group_name: Option<String>,
    participation_type: Option<String>,
    #[serde(default)]
    first_blood: bool,
    rule: RuleRequest,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleRequest {
    rule_type: String,
    ratio: Option<f64>,
    fixed_count: Option<i32>,
    rank_from: Option<i32>,
    rank_to: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerateRequest {
    resolver_run_id: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionRequest {
    expected_version: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualRecipientRequest {
    category_id: i64,
    team_id: i64,
    expected_set_version: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCategoryRequest {
    expected_version: i32,
    #[serde(flatten)]
    category: CategoryRequest,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CategoryResponse {
    id: i64,
    contest_id: i64,
    code: String,
    name: String,
    display_order: i32,
    include_star: bool,
    group_name: Option<String>,
    participation_type: Option<String>,
    first_blood: bool,
    version: i32,
    rule_type: String,
    ratio: Option<f64>,
    fixed_count: Option<i32>,
    rank_from: Option<i32>,
    rank_to: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RecipientResponse {
    id: i64,
    category_id: i64,
    category_code: String,
    category_name: String,
    team_id: i64,
    team_name: String,
    school: Option<String>,
    rank: Option<i32>,
    solved: Option<i32>,
    penalty_minutes: Option<i64>,
    participation_type: Option<String>,
    group_name: Option<String>,
    is_star: bool,
    is_manual: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AwardCandidateResponse {
    team_id: i64,
    team_name: String,
    school: Option<String>,
    rank: u32,
    participation_type: String,
    group_name: Option<String>,
    is_star: bool,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AwardResolverRunResponse {
    id: i64,
    #[serde(with = "time::serde::rfc3339")]
    completed_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AwardSetResponse {
    id: i64,
    contest_id: i64,
    resolver_run_id: i64,
    final_scoreboard_snapshot_id: i64,
    status: String,
    version: i32,
    #[serde(with = "time::serde::rfc3339")]
    generated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    frozen_at: Option<OffsetDateTime>,
    recipients: Vec<RecipientResponse>,
    conflicts: Vec<AwardConflict>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct AwardConflict {
    team_id: i64,
    team_name: String,
    category_codes: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationRequest {
    current_category_id: Option<i64>,
    status: String,
    #[serde(default)]
    auto_rotate: bool,
    interval_seconds: i32,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PresentationRecipient {
    id: i64,
    problem_id: Option<i64>,
    problem_alias: Option<String>,
    team_id: i64,
    team_name: String,
    school: Option<String>,
    seat_no: Option<String>,
    group_name: Option<String>,
    participation_type: Option<String>,
    star: bool,
    rank: Option<i32>,
    solved: Option<i32>,
    penalty_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PresentationCategory {
    id: i64,
    code: String,
    name: String,
    display_order: i32,
    group_name: Option<String>,
    first_blood: bool,
    #[sqlx(skip)]
    recipients: Vec<PresentationRecipient>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresentationResponse {
    contest_id: i64,
    contest_name: String,
    contest_status: String,
    #[serde(with = "time::serde::rfc3339")]
    server_time: OffsetDateTime,
    status: String,
    current_category_id: i64,
    auto_rotate: bool,
    interval_seconds: i32,
    #[serde(with = "time::serde::rfc3339")]
    state_updated_at: OffsetDateTime,
    categories: Vec<PresentationCategory>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostScriptSectionRequest {
    category_id: i64,
    cue_text: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostScriptRequest {
    opening_text: String,
    closing_text: String,
    sections: Vec<HostScriptSectionRequest>,
    expected_version: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HostScriptSectionResponse {
    category_id: i64,
    code: String,
    name: String,
    first_blood: bool,
    current: bool,
    cue_text: String,
    recipients: Vec<PresentationRecipient>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HostScriptResponse {
    contest_id: i64,
    contest_name: String,
    #[serde(with = "time::serde::rfc3339")]
    server_time: OffsetDateTime,
    presentation_status: String,
    current_category_id: i64,
    next_category_id: Option<i64>,
    auto_rotate: bool,
    interval_seconds: i32,
    #[serde(with = "time::serde::rfc3339")]
    state_updated_at: OffsetDateTime,
    version: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    updated_at: Option<OffsetDateTime>,
    opening_text: String,
    closing_text: String,
    sections: Vec<HostScriptSectionResponse>,
}

#[derive(Debug, sqlx::FromRow)]
struct CertificateRow {
    certificate_no: String,
    contest_id: i64,
    contest_name: String,
    award_code: String,
    award_name: String,
    problem_alias: Option<String>,
    team_id: i64,
    team_name: String,
    school: Option<String>,
    source_member_id: Option<i64>,
    recipient_name: String,
    recipient_role: Option<String>,
    seat_no: Option<String>,
    group_name: Option<String>,
    participation_type: Option<String>,
    rank: Option<i32>,
}

pub struct AwardService {
    database: PgPool,
}

impl AwardService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    async fn list_categories(
        &self,
        contest: i64,
        actor: &AuthUser,
    ) -> Result<Vec<CategoryResponse>, AppError> {
        require_operator(actor)?;
        category_query(&self.database, contest).await
    }

    async fn create_category(
        &self,
        contest: i64,
        request: CategoryRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<CategoryResponse, AppError> {
        require_operator(actor)?;
        let request = validate_category(request)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|e| AppError::internal("begin award category", e))?;
        ensure_awards_mutable(&mut tx, contest).await?;
        let id = sqlx::query_scalar::<_, i64>("INSERT INTO award_categories (contest_id, code, name, display_order, include_star, group_name, participation_type, first_blood) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
            .bind(contest).bind(&request.code).bind(&request.name).bind(request.display_order)
            .bind(request.include_star).bind(request.group_name.as_deref())
            .bind(request.participation_type.as_deref()).bind(request.first_blood)
            .fetch_one(&mut *tx).await.map_err(map_category_error)?;
        insert_rule(&mut tx, id, &request.rule).await?;
        audit(&mut tx, actor.id, "AWARD_CATEGORY_CREATED", id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit award category", e))?;
        load_category(&self.database, id).await
    }

    async fn update_category(
        &self,
        id: i64,
        request: UpdateCategoryRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<CategoryResponse, AppError> {
        require_operator(actor)?;
        let category = validate_category(request.category)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin award category update", error))?;
        let contest = sqlx::query_scalar::<_, i64>(
            "SELECT contest_id FROM award_categories WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock award category", error))?
        .ok_or_else(|| {
            AppError::not_found("AWARD_CATEGORY_NOT_FOUND", "Award category was not found")
        })?;
        ensure_awards_mutable(&mut tx, contest).await?;
        let changed = sqlx::query("UPDATE award_categories SET code=$2,name=$3,display_order=$4,include_star=$5,group_name=$6,participation_type=$7,first_blood=$8,updated_at=now(),version=version+1 WHERE id=$1 AND version=$9")
            .bind(id).bind(&category.code).bind(&category.name).bind(category.display_order)
            .bind(category.include_star).bind(category.group_name.as_deref())
            .bind(category.participation_type.as_deref()).bind(category.first_blood)
            .bind(request.expected_version).execute(&mut *tx).await.map_err(map_category_error)?
            .rows_affected();
        if changed != 1 {
            return Err(AppError::conflict(
                "AWARD_CATEGORY_VERSION_STALE",
                "Award category changed; reload and retry",
            ));
        }
        sqlx::query("DELETE FROM award_rules WHERE category_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("replace award category rule", error))?;
        insert_rule(&mut tx, id, &category.rule).await?;
        audit(&mut tx, actor.id, "AWARD_CATEGORY_UPDATED", id, ip).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit award category update", error))?;
        load_category(&self.database, id).await
    }

    async fn delete_category(
        &self,
        id: i64,
        expected_version: i32,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<(), AppError> {
        require_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin award category deletion", error))?;
        let (contest, version) = sqlx::query_as::<_, (i64, i32)>(
            "SELECT contest_id,version FROM award_categories WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock award category", error))?
        .ok_or_else(|| {
            AppError::not_found("AWARD_CATEGORY_NOT_FOUND", "Award category was not found")
        })?;
        ensure_awards_mutable(&mut tx, contest).await?;
        if version != expected_version {
            return Err(AppError::conflict(
                "AWARD_CATEGORY_VERSION_STALE",
                "Award category changed; reload and retry",
            ));
        }
        sqlx::query("DELETE FROM award_recipients WHERE category_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("delete category recipients", error))?;
        sqlx::query("DELETE FROM award_categories WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("delete award category", error))?;
        audit(&mut tx, actor.id, "AWARD_CATEGORY_DELETED", id, ip).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit award category deletion", error))?;
        Ok(())
    }

    async fn completed_resolver_runs(
        &self,
        contest: i64,
        actor: &AuthUser,
    ) -> Result<Vec<AwardResolverRunResponse>, AppError> {
        require_operator(actor)?;
        sqlx::query_as("SELECT id,completed_at FROM resolver_runs WHERE contest_id=$1 AND official AND status='COMPLETED' ORDER BY completed_at DESC,id DESC")
            .bind(contest).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list completed Resolver runs", error))
    }

    async fn candidates(
        &self,
        contest: i64,
        actor: &AuthUser,
    ) -> Result<Vec<AwardCandidateResponse>, AppError> {
        require_operator(actor)?;
        let snapshot = sqlx::query_scalar::<_, String>("SELECT snapshot.payload_json FROM award_sets award JOIN scoreboard_snapshots snapshot ON snapshot.id=award.final_scoreboard_snapshot_id WHERE award.contest_id=$1")
            .bind(contest).fetch_optional(&self.database).await
            .map_err(|error| AppError::internal("load award candidates", error))?
            .ok_or_else(|| AppError::not_found("AWARD_SET_NOT_FOUND", "Award set was not found"))?;
        let board: ScoreboardResponse = serde_json::from_str(&snapshot)
            .map_err(|error| AppError::internal("decode award candidates", error))?;
        Ok(board
            .rows
            .into_iter()
            .map(|row| AwardCandidateResponse {
                team_id: row.team_id,
                team_name: row.team_name,
                school: row.school,
                rank: row.rank,
                participation_type: row.participation_type,
                group_name: row.group_name,
                is_star: row.is_star,
            })
            .collect())
    }

    async fn generate(
        &self,
        contest: i64,
        run_id: i64,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AwardSetResponse, AppError> {
        require_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|e| AppError::internal("begin award generation", e))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("awards:{contest}"))
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal("lock award generation", e))?;
        let (snapshot_id, payload) = sqlx::query_as::<_, (i64, String)>(r#"
            SELECT run.source_final_snapshot_id, snapshot.payload_json
            FROM resolver_runs run JOIN scoreboard_snapshots snapshot ON snapshot.id = run.source_final_snapshot_id
            WHERE run.id = $1 AND run.contest_id = $2 AND run.official AND run.status = 'COMPLETED'
        "#).bind(run_id).bind(contest).fetch_optional(&mut *tx).await
            .map_err(|e| AppError::internal("load official Resolver result", e))?
            .ok_or_else(|| AppError::conflict("AWARD_RESOLVER_NOT_FINAL", "Awards require the completed official Resolver run"))?;
        let board: ScoreboardResponse = serde_json::from_str(&payload)
            .map_err(|e| AppError::internal("decode final award board", e))?;
        let categories = category_query_tx(&mut tx, contest).await?;
        if categories.is_empty() {
            return Err(AppError::validation(
                "categories",
                "must configure at least one award category",
            ));
        }
        let set_id = sqlx::query_scalar::<_, i64>("INSERT INTO award_sets (contest_id, resolver_run_id, final_scoreboard_snapshot_id, generated_by_user_id) VALUES ($1,$2,$3,$4) ON CONFLICT (contest_id) DO UPDATE SET resolver_run_id=excluded.resolver_run_id, final_scoreboard_snapshot_id=excluded.final_scoreboard_snapshot_id, generated_by_user_id=excluded.generated_by_user_id, generated_at=now(), version=award_sets.version+1 WHERE award_sets.status='DRAFT' RETURNING id")
            .bind(contest).bind(run_id).bind(snapshot_id).bind(actor.id).fetch_optional(&mut *tx).await
            .map_err(|e| AppError::internal("persist award set", e))?
            .ok_or_else(|| AppError::conflict("AWARD_SET_FROZEN", "Frozen awards cannot be regenerated"))?;
        sqlx::query("DELETE FROM award_recipients WHERE contest_id=$1 AND NOT is_manual")
            .bind(contest)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal("replace generated awards", e))?;
        for category in &categories {
            let mut eligible =
                board.rows.iter().filter(|row| eligible(row, category)).collect::<Vec<_>>();
            eligible.sort_by_key(|row| row.rank);
            let selected = select_rows(&eligible, category);
            for row in selected {
                insert_recipient(&mut tx, contest, category.id, row, snapshot_id, false).await?;
            }
        }
        audit(&mut tx, actor.id, "AWARDS_GENERATED", set_id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit award generation", e))?;
        self.load_set(contest, actor).await
    }

    async fn manual_add(
        &self,
        contest: i64,
        request: ManualRecipientRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AwardSetResponse, AppError> {
        require_operator(actor)?;
        let mut tx =
            self.database.begin().await.map_err(|e| AppError::internal("begin manual award", e))?;
        let (set_id, snapshot_id, version) = lock_set(&mut tx, contest).await?;
        if version != request.expected_set_version {
            return Err(stale());
        }
        let payload = sqlx::query_scalar::<_, String>(
            "SELECT payload_json FROM scoreboard_snapshots WHERE id=$1",
        )
        .bind(snapshot_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::internal("load award snapshot", e))?;
        let board: ScoreboardResponse = serde_json::from_str(&payload)
            .map_err(|e| AppError::internal("decode award snapshot", e))?;
        let row =
            board.rows.iter().find(|row| row.team_id == request.team_id).ok_or_else(|| {
                AppError::validation("teamId", "team is not present in the final snapshot")
            })?;
        let category_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM award_categories WHERE id=$1 AND contest_id=$2)",
        )
        .bind(request.category_id)
        .bind(contest)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::internal("check award category", e))?;
        if !category_exists {
            return Err(AppError::not_found(
                "AWARD_CATEGORY_NOT_FOUND",
                "Award category was not found",
            ));
        }
        insert_recipient(&mut tx, contest, request.category_id, row, snapshot_id, true).await?;
        sqlx::query("UPDATE award_sets SET version=version+1 WHERE id=$1")
            .bind(set_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal("version manual award", e))?;
        audit(&mut tx, actor.id, "AWARD_RECIPIENT_ADDED", set_id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit manual award", e))?;
        self.load_set(contest, actor).await
    }

    async fn manual_remove(
        &self,
        recipient: i64,
        expected_version: i32,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AwardSetResponse, AppError> {
        require_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin manual award removal", error))?;
        let (contest, manual) = sqlx::query_as::<_, (i64, bool)>(
            "SELECT contest_id,is_manual FROM award_recipients WHERE id=$1 FOR UPDATE",
        )
        .bind(recipient)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock award recipient", error))?
        .ok_or_else(|| {
            AppError::not_found("AWARD_RECIPIENT_NOT_FOUND", "Award recipient was not found")
        })?;
        let (set_id, _, version) = lock_set(&mut tx, contest).await?;
        if version != expected_version {
            return Err(stale());
        }
        if !manual {
            return Err(AppError::conflict(
                "AWARD_RECIPIENT_GENERATED",
                "Generated recipients must be changed by regenerating awards",
            ));
        }
        sqlx::query("DELETE FROM award_recipients WHERE id=$1")
            .bind(recipient)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("delete manual award recipient", error))?;
        sqlx::query("UPDATE award_sets SET version=version+1 WHERE id=$1")
            .bind(set_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("version manual award removal", error))?;
        audit(&mut tx, actor.id, "AWARD_RECIPIENT_REMOVED", recipient, ip).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit manual award removal", error))?;
        self.load_set(contest, actor).await
    }

    async fn freeze(
        &self,
        contest: i64,
        expected: i32,
        frozen: bool,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AwardSetResponse, AppError> {
        require_operator(actor)?;
        let mut tx =
            self.database.begin().await.map_err(|e| AppError::internal("begin award freeze", e))?;
        let changed = sqlx::query("UPDATE award_sets SET status=CASE WHEN $3 THEN 'FROZEN' ELSE 'DRAFT' END, frozen_at=CASE WHEN $3 THEN now() ELSE NULL END, frozen_by_user_id=CASE WHEN $3 THEN $4 ELSE NULL END, version=version+1 WHERE contest_id=$1 AND version=$2 AND status=CASE WHEN $3 THEN 'DRAFT' ELSE 'FROZEN' END")
            .bind(contest).bind(expected).bind(frozen).bind(actor.id).execute(&mut *tx).await
            .map_err(|e| AppError::internal("freeze award set", e))?.rows_affected();
        if changed != 1 {
            return Err(stale());
        }
        if frozen {
            snapshot_certificates(&mut tx, contest).await?;
        } else {
            sqlx::query("DELETE FROM award_certificate_rows WHERE contest_id=$1")
                .bind(contest)
                .execute(&mut *tx)
                .await
                .map_err(|error| AppError::internal("clear award certificate snapshot", error))?;
        }
        audit(
            &mut tx,
            actor.id,
            if frozen { "AWARDS_FROZEN" } else { "AWARDS_UNFROZEN" },
            contest,
            ip,
        )
        .await?;
        tx.commit().await.map_err(|e| AppError::internal("commit award freeze", e))?;
        self.load_set(contest, actor).await
    }

    async fn certificate_csv(
        &self,
        contest: i64,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<(String, String), AppError> {
        require_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin award certificate export", error))?;
        let contest_name = sqlx::query_scalar::<_, String>(
            "SELECT name FROM contests WHERE id=$1 AND deleted_at IS NULL",
        )
        .bind(contest)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("load certificate contest", error))?
        .ok_or_else(|| AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found"))?;
        let frozen = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM award_sets WHERE contest_id=$1 AND status='FROZEN')",
        )
        .bind(contest)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("check certificate award freeze", error))?;
        if !frozen {
            return Err(AppError::conflict(
                "AWARD_CERTIFICATE_EXPORT_NOT_FROZEN",
                "Freeze the award list before exporting certificates",
            ));
        }
        let row_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM award_certificate_rows WHERE contest_id=$1",
        )
        .bind(contest)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("count certificate snapshot", error))?;
        let recipient_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM award_recipients WHERE contest_id=$1",
        )
        .bind(contest)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("count certificate recipients", error))?;
        if row_count == 0 && recipient_count > 0 {
            // Compatibility backfill for contests frozen before certificate
            // snapshots were implemented by the Rust service.
            snapshot_certificates(&mut tx, contest).await?;
        }
        let rows = sqlx::query_as::<_, CertificateRow>(
            "SELECT certificate_no,contest_id,contest_name,award_code,award_name,problem_alias,team_id,team_name,school,source_member_id,recipient_name,recipient_role,seat_no,group_name,participation_type,rank FROM award_certificate_rows WHERE contest_id=$1 ORDER BY export_order",
        )
        .bind(contest)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| AppError::internal("load certificate snapshot", error))?;
        audit(&mut tx, actor.id, "AWARD_CERTIFICATES_EXPORTED", contest, ip).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit award certificate export", error))?;
        Ok((contest_name, certificate_csv(&rows)))
    }

    async fn load_set(&self, contest: i64, actor: &AuthUser) -> Result<AwardSetResponse, AppError> {
        require_operator(actor)?;
        let row = sqlx::query_as::<_, (i64,i64,i64,String,i32,OffsetDateTime,Option<OffsetDateTime>)>("SELECT id,resolver_run_id,final_scoreboard_snapshot_id,status,version,generated_at,frozen_at FROM award_sets WHERE contest_id=$1")
            .bind(contest).fetch_optional(&self.database).await.map_err(|e| AppError::internal("load award set", e))?
            .ok_or_else(|| AppError::not_found("AWARD_SET_NOT_FOUND", "Award set was not found"))?;
        let recipients = recipient_query(&self.database, contest).await?;
        let conflicts = conflicts(&recipients);
        Ok(AwardSetResponse {
            id: row.0,
            contest_id: contest,
            resolver_run_id: row.1,
            final_scoreboard_snapshot_id: row.2,
            status: row.3,
            version: row.4,
            generated_at: row.5,
            frozen_at: row.6,
            recipients,
            conflicts,
        })
    }

    async fn presentation(&self, contest: i64) -> Result<PresentationResponse, AppError> {
        let (contest_name, contest_status) = sqlx::query_as::<_, (String, String)>(
            "SELECT name,status FROM contests WHERE id=$1 AND deleted_at IS NULL AND visibility='PUBLIC'",
        )
        .bind(contest)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load award presentation contest", error))?
        .ok_or_else(|| AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found"))?;
        let frozen = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM award_sets WHERE contest_id=$1 AND status='FROZEN')",
        )
        .bind(contest)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("check frozen award presentation", error))?;
        if !frozen {
            return Err(AppError::not_found(
                "AWARD_PRESENTATION_NOT_READY",
                "A frozen award set is required",
            ));
        }
        let mut categories = sqlx::query_as::<_, PresentationCategory>(
            "SELECT id,code,name,display_order,group_name,first_blood FROM award_categories WHERE contest_id=$1 ORDER BY display_order,id",
        )
        .bind(contest)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("load award presentation categories", error))?;
        if categories.is_empty() {
            return Err(AppError::not_found(
                "AWARD_PRESENTATION_NOT_READY",
                "A frozen award set with categories is required",
            ));
        }
        // Load the category key alongside each public recipient, then group the
        // flattened rows into category sections.
        let recipients = sqlx::query_as::<_, (i64, i64, Option<i64>, Option<String>, i64, String, Option<String>, Option<String>, Option<String>, Option<String>, bool, Option<i32>, Option<i32>, Option<i64>)>(
            "SELECT category_id,id,problem_id,problem_alias,team_id,coalesce(team_name,''),school,seat_no,group_name,participation_type,is_star,rank,solved,penalty_minutes FROM award_recipients WHERE contest_id=$1 ORDER BY rank NULLS LAST,team_id,id",
        )
        .bind(contest)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("load award presentation recipients", error))?;
        for category in &mut categories {
            category.recipients = recipients
                .iter()
                .filter(|row| row.0 == category.id)
                .map(|row| PresentationRecipient {
                    id: row.1,
                    problem_id: row.2,
                    problem_alias: row.3.clone(),
                    team_id: row.4,
                    team_name: row.5.clone(),
                    school: row.6.clone(),
                    seat_no: row.7.clone(),
                    group_name: row.8.clone(),
                    participation_type: row.9.clone(),
                    star: row.10,
                    rank: row.11,
                    solved: row.12,
                    penalty_minutes: row.13,
                })
                .collect();
        }
        let state = sqlx::query_as::<_, (Option<i64>, String, bool, i32, OffsetDateTime)>(
            "SELECT current_category_id,status,auto_rotate,interval_seconds,updated_at FROM award_presentation_states WHERE contest_id=$1",
        )
        .bind(contest)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load award presentation state", error))?;
        let now = OffsetDateTime::now_utc();
        let first_category = categories[0].id;
        let (current, status, auto_rotate, interval, updated_at) = state.map_or(
            (first_category, "WAITING".to_owned(), false, 15, now),
            |(current, status, auto_rotate, interval, updated_at)| {
                let current = current
                    .filter(|id| categories.iter().any(|category| category.id == *id))
                    .unwrap_or(first_category);
                (current, status, auto_rotate, interval, updated_at)
            },
        );
        Ok(PresentationResponse {
            contest_id: contest,
            contest_name,
            contest_status,
            server_time: now,
            status,
            current_category_id: current,
            auto_rotate,
            interval_seconds: interval,
            state_updated_at: updated_at,
            categories,
        })
    }

    async fn update_presentation(
        &self,
        contest: i64,
        mut request: PresentationRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<PresentationResponse, AppError> {
        require_operator(actor)?;
        request.status = request.status.trim().to_ascii_uppercase();
        if !matches!(request.status.as_str(), "WAITING" | "PRESENTING" | "COMPLETED") {
            return Err(AppError::validation("status", "is not a presentation status"));
        }
        if !(5..=120).contains(&request.interval_seconds) {
            return Err(AppError::validation("intervalSeconds", "must be between 5 and 120"));
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin award presentation update", error))?;
        let category_ids = sqlx::query_scalar::<_, i64>(
            "SELECT c.id FROM award_categories c JOIN award_sets s ON s.contest_id=c.contest_id AND s.status='FROZEN' WHERE c.contest_id=$1 ORDER BY c.display_order,c.id",
        )
        .bind(contest)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| AppError::internal("load frozen presentation categories", error))?;
        let category_id = request
            .current_category_id
            .or_else(|| category_ids.first().copied())
            .ok_or_else(|| {
                AppError::conflict("AWARD_PRESENTATION_NOT_READY", "A frozen award set is required")
            })?;
        if !category_ids.contains(&category_id) {
            return Err(AppError::validation(
                "currentCategoryId",
                "AWARD_PRESENTATION_CATEGORY_NOT_FROZEN",
            ));
        }
        sqlx::query("INSERT INTO award_presentation_states(contest_id,current_category_id,status,auto_rotate,interval_seconds,updated_by_user_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(contest_id) DO UPDATE SET current_category_id=excluded.current_category_id,status=excluded.status,auto_rotate=excluded.auto_rotate,interval_seconds=excluded.interval_seconds,updated_by_user_id=excluded.updated_by_user_id,updated_at=now(),version=award_presentation_states.version+1")
            .bind(contest).bind(category_id).bind(&request.status).bind(request.auto_rotate).bind(request.interval_seconds).bind(actor.id)
            .execute(&mut *tx).await.map_err(|error| AppError::internal("save award presentation state", error))?;
        audit(&mut tx, actor.id, "AWARD_PRESENTATION_UPDATED", contest, ip).await?;
        sqlx::query("INSERT INTO realtime_outbox(event_id,contest_id,event_type,scope,payload_json) VALUES($1,$2,'AWARDS_UPDATED','PUBLIC',$3)")
            .bind(uuid::Uuid::new_v4()).bind(contest).bind(serde_json::json!({"categoryId":category_id,"status":request.status}))
            .execute(&mut *tx).await.map_err(|error| AppError::internal("publish award presentation update", error))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit award presentation update", error))?;
        self.presentation(contest).await
    }

    async fn host_script(&self, contest: i64) -> Result<HostScriptResponse, AppError> {
        let presentation = self.presentation(contest).await.map_err(map_host_script_not_ready)?;
        self.shape_host_script(presentation).await
    }

    async fn save_host_script(
        &self,
        contest: i64,
        request: HostScriptRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<HostScriptResponse, AppError> {
        require_operator(actor)?;
        if request.opening_text.chars().count() > 4000
            || request.closing_text.chars().count() > 4000
            || request.sections.len() > 100
            || request.sections.iter().any(|section| section.cue_text.chars().count() > 2000)
        {
            return Err(AppError::validation("hostScript", "contains text over the size limit"));
        }
        let presentation = self.presentation(contest).await.map_err(map_host_script_not_ready)?;
        let category_ids =
            presentation.categories.iter().map(|category| category.id).collect::<Vec<_>>();
        let mut cues = std::collections::HashMap::new();
        for section in request.sections {
            if !category_ids.contains(&section.category_id) {
                return Err(AppError::validation(
                    "categoryId",
                    "AWARD_HOST_SCRIPT_CATEGORY_INVALID",
                ));
            }
            if cues.insert(section.category_id, section.cue_text).is_some() {
                return Err(AppError::validation(
                    "categoryId",
                    "AWARD_HOST_SCRIPT_CATEGORY_DUPLICATE",
                ));
            }
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin host script update", error))?;
        let existing = sqlx::query_as::<_, (i64, i64)>(
            "SELECT id,version FROM award_host_scripts WHERE contest_id=$1 FOR UPDATE",
        )
        .bind(contest)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock host script", error))?;
        let script_id = match existing {
            None if request.expected_version.is_none() => sqlx::query_scalar::<_, i64>("INSERT INTO award_host_scripts(contest_id,opening_text,closing_text,updated_by_user_id) VALUES($1,$2,$3,$4) RETURNING id")
                .bind(contest).bind(&request.opening_text).bind(&request.closing_text).bind(actor.id).fetch_one(&mut *tx).await.map_err(|error| AppError::internal("create host script", error))?,
            Some((id, version)) if request.expected_version == Some(version) => {
                sqlx::query("UPDATE award_host_scripts SET opening_text=$2,closing_text=$3,updated_by_user_id=$4,updated_at=now(),version=version+1 WHERE id=$1")
                    .bind(id).bind(&request.opening_text).bind(&request.closing_text).bind(actor.id).execute(&mut *tx).await.map_err(|error| AppError::internal("update host script", error))?;
                id
            }
            _ => return Err(AppError::conflict("AWARD_HOST_SCRIPT_VERSION_CONFLICT", "Host script changed; reload and retry")),
        };
        sqlx::query("DELETE FROM award_host_script_sections WHERE host_script_id=$1")
            .bind(script_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("replace host script sections", error))?;
        for (index, category) in presentation.categories.iter().enumerate() {
            let cue = cues.remove(&category.id).unwrap_or_else(|| default_cue(category));
            sqlx::query("INSERT INTO award_host_script_sections(host_script_id,category_id,cue_text,display_order) VALUES($1,$2,$3,$4)")
                .bind(script_id).bind(category.id).bind(cue).bind(i32::try_from(index + 1).unwrap_or(i32::MAX)).execute(&mut *tx).await.map_err(|error| AppError::internal("save host script section", error))?;
        }
        audit(&mut tx, actor.id, "AWARD_HOST_SCRIPT_UPDATED", contest, ip).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit host script update", error))?;
        self.shape_host_script(presentation).await
    }

    async fn shape_host_script(
        &self,
        presentation: PresentationResponse,
    ) -> Result<HostScriptResponse, AppError> {
        let script = sqlx::query_as::<_, (i64, String, String, i64, OffsetDateTime)>("SELECT id,opening_text,closing_text,version,updated_at FROM award_host_scripts WHERE contest_id=$1")
            .bind(presentation.contest_id).fetch_optional(&self.database).await.map_err(|error| AppError::internal("load host script", error))?;
        let cues = if let Some((id, _, _, _, _)) = &script {
            sqlx::query_as::<_, (i64, String)>("SELECT category_id,cue_text FROM award_host_script_sections WHERE host_script_id=$1 ORDER BY display_order")
                .bind(id).fetch_all(&self.database).await.map_err(|error| AppError::internal("load host script sections", error))?.into_iter().collect::<std::collections::HashMap<_, _>>()
        } else {
            std::collections::HashMap::new()
        };
        let current_index = presentation
            .categories
            .iter()
            .position(|category| category.id == presentation.current_category_id)
            .unwrap_or(0);
        let next_category_id =
            presentation.categories.get(current_index + 1).map(|category| category.id);
        let sections = presentation
            .categories
            .iter()
            .map(|category| HostScriptSectionResponse {
                category_id: category.id,
                code: category.code.clone(),
                name: category.name.clone(),
                first_blood: category.first_blood,
                current: category.id == presentation.current_category_id,
                cue_text: cues.get(&category.id).cloned().unwrap_or_else(|| default_cue(category)),
                recipients: category.recipients.clone(),
            })
            .collect();
        let (version, updated_at, opening_text, closing_text) = script.map_or_else(
            || {
                (
                    None,
                    None,
                    format!("各位嘉宾、参赛选手，{}颁奖典礼现在开始。", presentation.contest_name),
                    "祝贺所有获奖队伍，感谢各位嘉宾与参赛选手。颁奖典礼到此结束。".to_owned(),
                )
            },
            |(_, opening, closing, version, updated)| {
                (Some(version), Some(updated), opening, closing)
            },
        );
        Ok(HostScriptResponse {
            contest_id: presentation.contest_id,
            contest_name: presentation.contest_name,
            server_time: presentation.server_time,
            presentation_status: presentation.status,
            current_category_id: presentation.current_category_id,
            next_category_id,
            auto_rotate: presentation.auto_rotate,
            interval_seconds: presentation.interval_seconds,
            state_updated_at: presentation.state_updated_at,
            version,
            updated_at,
            opening_text,
            closing_text,
            sections,
        })
    }
}

async fn snapshot_certificates(
    tx: &mut Transaction<'_, Postgres>,
    contest: i64,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM award_certificate_rows WHERE contest_id=$1")
        .bind(contest)
        .execute(&mut **tx)
        .await
        .map_err(|error| AppError::internal("replace award certificate snapshot", error))?;
    sqlx::query(
        r#"
        INSERT INTO award_certificate_rows(
            contest_id,award_recipient_id,source_member_id,certificate_no,export_order,
            contest_name,award_code,award_name,problem_alias,team_id,team_name,school,
            recipient_name,recipient_role,seat_no,group_name,participation_type,rank
        )
        SELECT
            recipient.contest_id,
            recipient.id,
            member.id,
            'XCPC-' || recipient.contest_id || '-R' || recipient.id || '-' ||
                CASE WHEN member.id IS NULL THEN 'TEAM' ELSE 'M' || member.id END,
            row_number() OVER (
                ORDER BY category.display_order,
                    recipient.rank NULLS LAST,
                    recipient.team_id,
                    recipient.id,
                    member.created_at NULLS FIRST,
                    member.id NULLS FIRST
            )::integer,
            contest.name,
            category.code,
            category.name,
            recipient.problem_alias,
            recipient.team_id,
            coalesce(recipient.team_name, ''),
            recipient.school,
            coalesce(member.name, recipient.team_name, ''),
            coalesce(member.role_name, 'TEAM'),
            recipient.seat_no,
            recipient.group_name,
            recipient.participation_type,
            recipient.rank
        FROM award_recipients recipient
        JOIN contests contest ON contest.id=recipient.contest_id AND contest.deleted_at IS NULL
        JOIN award_categories category ON category.id=recipient.category_id
        LEFT JOIN team_members member ON member.team_id=recipient.team_id
        WHERE recipient.contest_id=$1
        "#,
    )
    .bind(contest)
    .execute(&mut **tx)
    .await
    .map_err(|error| AppError::internal("snapshot award certificates", error))?;
    Ok(())
}

fn certificate_csv(rows: &[CertificateRow]) -> String {
    let mut output = String::from(
        "\u{feff}证书编号,比赛编号,比赛名称,奖项代码,奖项名称,题目标识,队伍编号,队伍名称,学校,成员编号,获奖人,成员角色,座位号,组别,参赛类型,名次\r\n",
    );
    for row in rows {
        let fields = [
            certificate_value(Some(&row.certificate_no)),
            row.contest_id.to_string(),
            certificate_value(Some(&row.contest_name)),
            certificate_value(Some(&row.award_code)),
            certificate_value(Some(&row.award_name)),
            certificate_value(row.problem_alias.as_deref()),
            row.team_id.to_string(),
            certificate_value(Some(&row.team_name)),
            certificate_value(row.school.as_deref()),
            row.source_member_id.map_or_else(String::new, |value| value.to_string()),
            certificate_value(Some(&row.recipient_name)),
            certificate_value(row.recipient_role.as_deref()),
            certificate_value(row.seat_no.as_deref()),
            certificate_value(row.group_name.as_deref()),
            certificate_value(row.participation_type.as_deref()),
            row.rank.map_or_else(String::new, |value| value.to_string()),
        ];
        output.push_str(&fields.join(","));
        output.push_str("\r\n");
    }
    output
}

fn certificate_value(value: Option<&str>) -> String {
    let value = value.unwrap_or("");
    let safe = if matches!(value.as_bytes().first(), Some(b'=' | b'+' | b'-' | b'@')) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if safe.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", safe.replace('"', "\"\""))
    } else {
        safe
    }
}

fn default_cue(category: &PresentationCategory) -> String {
    let verb = if category.first_blood { "公布" } else { "颁发" };
    format!("接下来{verb}{}，请获奖队伍代表上台领奖。", category.name)
}

fn map_host_script_not_ready(error: AppError) -> AppError {
    if error.code() == "AWARD_PRESENTATION_NOT_READY" {
        AppError::conflict("AWARD_HOST_SCRIPT_NOT_READY", "A frozen award set is required")
    } else {
        error
    }
}

fn validate_category(mut r: CategoryRequest) -> Result<CategoryRequest, AppError> {
    r.code = r.code.trim().to_ascii_uppercase();
    r.name = r.name.trim().to_owned();
    r.group_name = r.group_name.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    r.participation_type = r.participation_type.map(|v| v.trim().to_ascii_uppercase());
    if r.code.is_empty()
        || r.code.len() > 64
        || !r.code.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        || r.name.is_empty()
        || r.name.chars().count() > 128
        || !(1..=1000).contains(&r.display_order)
    {
        return Err(AppError::validation(
            "category",
            "contains invalid code, name, or displayOrder",
        ));
    }
    if r.participation_type
        .as_ref()
        .is_some_and(|v| !matches!(v.as_str(), "OFFICIAL" | "STAR" | "PRACTICE"))
    {
        return Err(AppError::validation(
            "participationType",
            "must be OFFICIAL, STAR, or PRACTICE",
        ));
    }
    r.rule.rule_type = r.rule.rule_type.to_ascii_uppercase();
    let valid = match r.rule.rule_type.as_str() {
        "FIXED_COUNT" => {
            r.rule.fixed_count.is_some_and(|v| v > 0)
                && r.rule.ratio.is_none()
                && r.rule.rank_from.is_none()
                && r.rule.rank_to.is_none()
        }
        "RATIO" => {
            r.rule.ratio.is_some_and(|v| v > 0.0 && v <= 1.0)
                && r.rule.fixed_count.is_none()
                && r.rule.rank_from.is_none()
                && r.rule.rank_to.is_none()
        }
        "RANK_RANGE" => {
            r.rule.rank_from.zip(r.rule.rank_to).is_some_and(|(a, b)| a > 0 && b >= a)
                && r.rule.fixed_count.is_none()
                && r.rule.ratio.is_none()
        }
        _ => false,
    };
    if !valid {
        return Err(AppError::validation("rule", "contains an invalid award rule"));
    }
    Ok(r)
}

async fn insert_rule(
    tx: &mut Transaction<'_, Postgres>,
    category: i64,
    r: &RuleRequest,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO award_rules(category_id,rule_type,ratio,fixed_count,rank_from,rank_to) VALUES($1,$2,$3,$4,$5,$6)").bind(category).bind(&r.rule_type).bind(r.ratio).bind(r.fixed_count).bind(r.rank_from).bind(r.rank_to).execute(&mut**tx).await.map(|_|()).map_err(|e|AppError::internal("insert award rule",e))
}

const CATEGORY_SQL: &str = "SELECT c.id,c.contest_id,c.code,c.name,c.display_order,c.include_star,c.group_name,c.participation_type,c.first_blood,c.version,r.rule_type,r.ratio::float8 AS ratio,r.fixed_count,r.rank_from,r.rank_to FROM award_categories c JOIN award_rules r ON r.category_id=c.id";
async fn category_query(db: &PgPool, c: i64) -> Result<Vec<CategoryResponse>, AppError> {
    sqlx::query_as(&format!("{CATEGORY_SQL} WHERE c.contest_id=$1 ORDER BY c.display_order,c.id"))
        .bind(c)
        .fetch_all(db)
        .await
        .map_err(|e| AppError::internal("list award categories", e))
}
async fn category_query_tx(
    tx: &mut Transaction<'_, Postgres>,
    c: i64,
) -> Result<Vec<CategoryResponse>, AppError> {
    sqlx::query_as(&format!("{CATEGORY_SQL} WHERE c.contest_id=$1 ORDER BY c.display_order,c.id"))
        .bind(c)
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| AppError::internal("list award categories", e))
}
async fn load_category(db: &PgPool, id: i64) -> Result<CategoryResponse, AppError> {
    sqlx::query_as(&format!("{CATEGORY_SQL} WHERE c.id=$1"))
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::internal("load award category", e))?
        .ok_or_else(|| {
            AppError::not_found("AWARD_CATEGORY_NOT_FOUND", "Award category was not found")
        })
}

fn eligible(row: &ScoreboardRow, c: &CategoryResponse) -> bool {
    (c.include_star || !row.is_star)
        && c.group_name.as_ref().is_none_or(|g| row.group_name.as_ref() == Some(g))
        && c.participation_type.as_ref().is_none_or(|p| &row.participation_type == p)
}
fn select_rows<'a>(rows: &[&'a ScoreboardRow], c: &CategoryResponse) -> Vec<&'a ScoreboardRow> {
    if c.first_blood {
        return rows
            .iter()
            .filter(|row| row.problems.iter().any(|cell| cell.first_blood))
            .copied()
            .collect();
    }
    match c.rule_type.as_str() {
        "FIXED_COUNT" => rows.iter().take(c.fixed_count.unwrap_or(0) as usize).copied().collect(),
        "RATIO" => {
            let n = ((rows.len() as f64) * c.ratio.unwrap_or(0.0)).ceil() as usize;
            rows.iter().take(n).copied().collect()
        }
        "RANK_RANGE" => rows
            .iter()
            .filter(|r| {
                c.rank_from.is_some_and(|a| r.rank >= a as u32)
                    && c.rank_to.is_some_and(|b| r.rank <= b as u32)
            })
            .copied()
            .collect(),
        _ => Vec::new(),
    }
}

async fn insert_recipient(
    tx: &mut Transaction<'_, Postgres>,
    contest: i64,
    category: i64,
    row: &ScoreboardRow,
    snapshot: i64,
    manual: bool,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO award_recipients(contest_id,category_id,team_id,rank,solved,penalty_minutes,team_name,school,group_name,is_star,is_manual,participation_type,source_scoreboard_snapshot_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(contest_id,category_id,team_id,award_key) DO UPDATE SET rank=excluded.rank,solved=excluded.solved,penalty_minutes=excluded.penalty_minutes,team_name=excluded.team_name,school=excluded.school,group_name=excluded.group_name,is_star=excluded.is_star,is_manual=award_recipients.is_manual OR excluded.is_manual,participation_type=excluded.participation_type,source_scoreboard_snapshot_id=excluded.source_scoreboard_snapshot_id,updated_at=now(),version=award_recipients.version+1").bind(contest).bind(category).bind(row.team_id).bind(i32::try_from(row.rank).unwrap_or(i32::MAX)).bind(row.solved_count).bind(row.penalty_minutes).bind(&row.team_name).bind(row.school.as_deref()).bind(row.group_name.as_deref()).bind(row.is_star).bind(manual).bind(&row.participation_type).bind(snapshot).execute(&mut**tx).await.map(|_|()).map_err(|e|AppError::internal("insert award recipient",e))
}

const RECIPIENT_SQL: &str = "SELECT r.id,r.category_id,c.code AS category_code,c.name AS category_name,r.team_id,coalesce(r.team_name,'') AS team_name,r.school,r.rank,r.solved,r.penalty_minutes,r.participation_type,r.group_name,r.is_star,r.is_manual FROM award_recipients r JOIN award_categories c ON c.id=r.category_id WHERE r.contest_id=$1 ORDER BY c.display_order,r.rank NULLS LAST,r.team_id";
async fn recipient_query(db: &PgPool, c: i64) -> Result<Vec<RecipientResponse>, AppError> {
    sqlx::query_as(RECIPIENT_SQL)
        .bind(c)
        .fetch_all(db)
        .await
        .map_err(|e| AppError::internal("list award recipients", e))
}
fn conflicts(rows: &[RecipientResponse]) -> Vec<AwardConflict> {
    let mut map = std::collections::BTreeMap::<i64, (String, Vec<String>)>::new();
    for r in rows {
        let e = map.entry(r.team_id).or_insert_with(|| (r.team_name.clone(), Vec::new()));
        e.1.push(r.category_code.clone());
    }
    map.into_iter()
        .filter(|(_, (_, c))| c.len() > 1)
        .map(|(team_id, (team_name, category_codes))| AwardConflict {
            team_id,
            team_name,
            category_codes,
        })
        .collect()
}

async fn ensure_awards_mutable(
    tx: &mut Transaction<'_, Postgres>,
    contest: i64,
) -> Result<(), AppError> {
    let frozen = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM award_sets WHERE contest_id=$1 AND status='FROZEN')",
    )
    .bind(contest)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| AppError::internal("check award freeze", e))?;
    if frozen {
        Err(AppError::conflict("AWARD_SET_FROZEN", "Frozen awards cannot be changed"))
    } else {
        Ok(())
    }
}
async fn lock_set(
    tx: &mut Transaction<'_, Postgres>,
    contest: i64,
) -> Result<(i64, i64, i32), AppError> {
    sqlx::query_as("SELECT id,final_scoreboard_snapshot_id,version FROM award_sets WHERE contest_id=$1 AND status='DRAFT' FOR UPDATE").bind(contest).fetch_optional(&mut**tx).await.map_err(|e|AppError::internal("lock award set",e))?.ok_or_else(||AppError::conflict("AWARD_SET_NOT_MUTABLE","A draft award set is required"))
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: i64,
    action: &str,
    id: i64,
    ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs(actor_user_id,action,target_type,target_id,request_ip,result)VALUES($1,$2,'AWARD',$3,$4,'success')").bind(actor).bind(action).bind(id.to_string()).bind(ip.to_string()).execute(&mut**tx).await.map(|_|()).map_err(|e|AppError::internal("record award audit",e))
}
fn require_operator(a: &AuthUser) -> Result<(), AppError> {
    if a.has_role("SUPER_ADMIN") || a.has_role("AWARD_OPERATOR") {
        Ok(())
    } else {
        Err(AppError::forbidden("AWARD_OPERATOR_REQUIRED", "Award operator role is required"))
    }
}
fn stale() -> AppError {
    AppError::conflict("AWARD_VERSION_STALE", "Award set changed; reload and retry")
}
fn map_category_error(e: sqlx::Error) -> AppError {
    if e.as_database_error().and_then(sqlx::error::DatabaseError::constraint).is_some() {
        AppError::conflict("AWARD_CATEGORY_CONFLICT", "Award code or display order is already used")
    } else {
        AppError::internal("create award category", e)
    }
}

#[utoipa::path(get, path = "/api/admin/contests/{contest_id}/award-categories", operation_id = "listAwardCategories", tag = "awards", params(("contest_id" = i64, Path)), responses((status = 200, body = [CategoryResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_categories(
    c: AuthContext,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<CategoryResponse>>, AppError> {
    c.require_password_ready()?;
    Ok(Json(s.awards().list_categories(id, c.user()).await?))
}
#[utoipa::path(post, path = "/api/admin/contests/{contest_id}/award-categories", operation_id = "createAwardCategory", tag = "awards", params(("contest_id" = i64, Path)), request_body = CategoryRequest, responses((status = 200, body = CategoryResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create_category(
    c: AuthContext,
    State(s): State<AppState>,
    ConnectInfo(p): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<CategoryRequest>, JsonRejection>,
) -> Result<Json<CategoryResponse>, AppError> {
    c.require_password_ready()?;
    let Json(r) = payload.map_err(|_| AppError::validation("request", "invalid award category"))?;
    Ok(Json(s.awards().create_category(id, r, c.user(), p.ip()).await?))
}
#[utoipa::path(put, path = "/api/admin/award-categories/{id}", operation_id = "updateAwardCategory", tag = "awards", params(("id" = i64, Path)), request_body = UpdateCategoryRequest, responses((status = 200, body = CategoryResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_category(
    c: AuthContext,
    State(s): State<AppState>,
    ConnectInfo(p): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<UpdateCategoryRequest>, JsonRejection>,
) -> Result<Json<CategoryResponse>, AppError> {
    c.require_password_ready()?;
    let Json(r) =
        payload.map_err(|_| AppError::validation("request", "invalid award category update"))?;
    Ok(Json(s.awards().update_category(id, r, c.user(), p.ip()).await?))
}
#[utoipa::path(delete, path = "/api/admin/award-categories/{id}", operation_id = "deleteAwardCategory", tag = "awards", params(("id" = i64, Path)), request_body = VersionRequest, responses((status = 204), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn delete_category(
    c: AuthContext,
    State(s): State<AppState>,
    ConnectInfo(p): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<StatusCode, AppError> {
    c.require_password_ready()?;
    let Json(r) =
        payload.map_err(|_| AppError::validation("request", "must contain expectedVersion"))?;
    s.awards().delete_category(id, r.expected_version, c.user(), p.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(get, path = "/api/admin/contests/{contest_id}/awards/resolver-runs", operation_id = "listAwardResolverRuns", tag = "awards", params(("contest_id" = i64, Path)), responses((status = 200, body = [AwardResolverRunResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn completed_resolver_runs(
    c: AuthContext,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<AwardResolverRunResponse>>, AppError> {
    c.require_password_ready()?;
    Ok(Json(s.awards().completed_resolver_runs(id, c.user()).await?))
}
#[utoipa::path(post, path = "/api/admin/contests/{contest_id}/awards", operation_id = "generateAwards", tag = "awards", params(("contest_id" = i64, Path)), request_body = GenerateRequest, responses((status = 200, body = AwardSetResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn generate(
    c: AuthContext,
    State(s): State<AppState>,
    ConnectInfo(p): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<GenerateRequest>, JsonRejection>,
) -> Result<Json<AwardSetResponse>, AppError> {
    c.require_password_ready()?;
    let Json(r) =
        payload.map_err(|_| AppError::validation("request", "must contain resolverRunId"))?;
    Ok(Json(s.awards().generate(id, r.resolver_run_id, c.user(), p.ip()).await?))
}
#[utoipa::path(get, path = "/api/admin/contests/{contest_id}/awards", operation_id = "getAwards", tag = "awards", params(("contest_id" = i64, Path)), responses((status = 200, body = AwardSetResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get(
    c: AuthContext,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AwardSetResponse>, AppError> {
    c.require_password_ready()?;
    Ok(Json(s.awards().load_set(id, c.user()).await?))
}
#[utoipa::path(post, path = "/api/admin/contests/{contest_id}/awards/manual", operation_id = "addManualAwardRecipient", tag = "awards", params(("contest_id" = i64, Path)), request_body = ManualRecipientRequest, responses((status = 200, body = AwardSetResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn manual_add(
    c: AuthContext,
    State(s): State<AppState>,
    ConnectInfo(p): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<ManualRecipientRequest>, JsonRejection>,
) -> Result<Json<AwardSetResponse>, AppError> {
    c.require_password_ready()?;
    let Json(r) =
        payload.map_err(|_| AppError::validation("request", "invalid manual recipient"))?;
    Ok(Json(s.awards().manual_add(id, r, c.user(), p.ip()).await?))
}
#[utoipa::path(get, path = "/api/admin/contests/{contest_id}/awards/candidates", operation_id = "listAwardCandidates", tag = "awards", params(("contest_id" = i64, Path)), responses((status = 200, body = [AwardCandidateResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn candidates(
    c: AuthContext,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<AwardCandidateResponse>>, AppError> {
    c.require_password_ready()?;
    Ok(Json(s.awards().candidates(id, c.user()).await?))
}
#[utoipa::path(delete, path = "/api/admin/award-recipients/{id}", operation_id = "removeManualAwardRecipient", tag = "awards", params(("id" = i64, Path)), request_body = VersionRequest, responses((status = 200, body = AwardSetResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn manual_remove(
    c: AuthContext,
    State(s): State<AppState>,
    ConnectInfo(p): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<Json<AwardSetResponse>, AppError> {
    c.require_password_ready()?;
    let Json(r) =
        payload.map_err(|_| AppError::validation("request", "must contain expectedVersion"))?;
    Ok(Json(s.awards().manual_remove(id, r.expected_version, c.user(), p.ip()).await?))
}
async fn freeze_command(
    c: AuthContext,
    s: AppState,
    p: SocketAddr,
    id: i64,
    payload: Result<Json<VersionRequest>, JsonRejection>,
    frozen: bool,
) -> Result<Json<AwardSetResponse>, AppError> {
    c.require_password_ready()?;
    let Json(r) =
        payload.map_err(|_| AppError::validation("request", "must contain expectedVersion"))?;
    Ok(Json(s.awards().freeze(id, r.expected_version, frozen, c.user(), p.ip()).await?))
}
#[utoipa::path(post, path = "/api/admin/contests/{contest_id}/awards/freeze", operation_id = "freezeAwards", tag = "awards", params(("contest_id" = i64, Path)), request_body = VersionRequest, responses((status = 200, body = AwardSetResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn freeze(
    c: AuthContext,
    State(s): State<AppState>,
    ConnectInfo(p): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<Json<AwardSetResponse>, AppError> {
    freeze_command(c, s, p, id, payload, true).await
}
#[utoipa::path(post, path = "/api/admin/contests/{contest_id}/awards/unfreeze", operation_id = "unfreezeAwards", tag = "awards", params(("contest_id" = i64, Path)), request_body = VersionRequest, responses((status = 200, body = AwardSetResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn unfreeze(
    c: AuthContext,
    State(s): State<AppState>,
    ConnectInfo(p): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<Json<AwardSetResponse>, AppError> {
    freeze_command(c, s, p, id, payload, false).await
}
#[utoipa::path(get, path = "/api/admin/contests/{contest_id}/awards.csv", operation_id = "exportAwardsCsv", tag = "awards", params(("contest_id" = i64, Path)), responses((status = 200, body = String, content_type = "text/csv"), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn csv(
    c: AuthContext,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    c.require_password_ready()?;
    let set = s.awards().load_set(id, c.user()).await?;
    let mut out = String::from(
        "categoryCode,categoryName,rank,teamId,teamName,school,participationType,groupName,manual\n",
    );
    for r in set.recipients {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            csv_field(&r.category_code),
            csv_field(&r.category_name),
            r.rank.map_or_else(String::new, |v| v.to_string()),
            r.team_id,
            csv_field(&r.team_name),
            csv_field(r.school.as_deref().unwrap_or("")),
            r.participation_type.unwrap_or_default(),
            csv_field(r.group_name.as_deref().unwrap_or("")),
            r.is_manual
        ));
    }
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/csv; charset=utf-8")),
            (
                header::CONTENT_DISPOSITION,
                HeaderValue::from_static("attachment; filename=awards.csv"),
            ),
        ],
        out,
    )
        .into_response())
}

#[utoipa::path(get, path = "/api/public/contests/{contest_id}/awards/presentation", operation_id = "getPublicAwardPresentation", tag = "awards", params(("contest_id" = i64, Path)), responses((status = 200, body = PresentationResponse), (status = 404, body = crate::error::ApiErrorBody)))]
pub async fn public_presentation(
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<PresentationResponse>, AppError> {
    Ok(Json(state.awards().presentation(contest_id).await?))
}

#[utoipa::path(put, path = "/api/contests/{contest_id}/awards/presentation", operation_id = "updateAwardPresentation", tag = "awards", params(("contest_id" = i64, Path)), request_body = PresentationRequest, responses((status = 200, body = PresentationResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_presentation(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<PresentationRequest>, JsonRejection>,
) -> Result<Json<PresentationResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid presentation state"))?;
    Ok(Json(
        state.awards().update_presentation(contest_id, request, context.user(), peer.ip()).await?,
    ))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/awards/host-script", operation_id = "getAwardHostScript", tag = "awards", params(("contest_id" = i64, Path)), responses((status = 200, body = HostScriptResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get_host_script(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<HostScriptResponse>, AppError> {
    context.require_password_ready()?;
    require_operator(context.user())?;
    Ok(Json(state.awards().host_script(contest_id).await?))
}

#[utoipa::path(put, path = "/api/contests/{contest_id}/awards/host-script", operation_id = "saveAwardHostScript", tag = "awards", params(("contest_id" = i64, Path)), request_body = HostScriptRequest, responses((status = 200, body = HostScriptResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn save_host_script(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<HostScriptRequest>, JsonRejection>,
) -> Result<Json<HostScriptResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid host script"))?;
    Ok(Json(state.awards().save_host_script(contest_id, request, context.user(), peer.ip()).await?))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/awards/certificates/export", operation_id = "exportAwardCertificates", tag = "awards", params(("contest_id" = i64, Path)), responses((status = 200, body = String, content_type = "text/csv"), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn certificate_export(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
) -> Result<Response, AppError> {
    context.require_password_ready()?;
    let (contest_name, csv) =
        state.awards().certificate_csv(contest_id, context.user(), peer.ip()).await?;
    let encoded_name = percent_encode_filename(&format!("{contest_name}-证书数据.csv"));
    let disposition = HeaderValue::from_str(&format!(
        "attachment; filename=\"certificates-contest-{contest_id}.csv\"; filename*=UTF-8''{encoded_name}"
    ))
    .map_err(|error| AppError::internal("build certificate download header", error))?;
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/csv; charset=utf-8")),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        csv,
    )
        .into_response())
}

fn percent_encode_filename(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}

fn csv_field(v: &str) -> String {
    let safe = if matches!(v.as_bytes().first(), Some(b'=' | b'+' | b'-' | b'@')) {
        format!("'{v}")
    } else {
        v.to_owned()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use sha2::{Digest, Sha256};
    use sqlx::PgPool;
    use time::OffsetDateTime;

    use super::*;
    use crate::features::{auth::model::UserType, scoreboard::ScoreboardRow};
    #[test]
    fn rules_are_closed() {
        assert!(
            validate_category(CategoryRequest {
                code: "gold".into(),
                name: "Gold".into(),
                display_order: 1,
                include_star: false,
                group_name: None,
                participation_type: Some("official".into()),
                first_blood: false,
                rule: RuleRequest {
                    rule_type: "fixed_count".into(),
                    ratio: None,
                    fixed_count: Some(3),
                    rank_from: None,
                    rank_to: None
                }
            })
            .is_ok()
        );
    }

    #[test]
    fn award_csv_blocks_spreadsheet_formulas_and_escapes_quotes() {
        assert_eq!(csv_field("=cmd()"), "\"'=cmd()\"");
        assert_eq!(csv_field("A \"Team\""), "\"A \"\"Team\"\"\"");
    }

    #[test]
    fn certificate_csv_is_excel_safe_and_filename_is_rfc5987_compatible() {
        assert_eq!(certificate_value(Some("=cmd()")), "'=cmd()");
        assert_eq!(certificate_value(Some("Alice, A")), "\"Alice, A\"");
        assert_eq!(
            percent_encode_filename("华东-证书.csv"),
            "%E5%8D%8E%E4%B8%9C-%E8%AF%81%E4%B9%A6.csv"
        );
    }

    #[test]
    fn first_blood_category_uses_snapshot_cell_markers_instead_of_rank_rule() {
        let mut first = board_row_for_award_test(1, true);
        let second = board_row_for_award_test(2, false);
        first.problems[0].first_blood = true;
        let category = category_response_for_award_test(true);
        let rows = vec![&first, &second];
        let selected = select_rows(&rows, &category);
        assert_eq!(selected.iter().map(|row| row.team_id).collect::<Vec<_>>(), vec![1]);
    }

    fn board_row_for_award_test(team_id: i64, solved: bool) -> ScoreboardRow {
        ScoreboardRow {
            rank: u32::try_from(team_id).expect("rank"),
            official_rank: Some(u32::try_from(team_id).expect("official rank")),
            team_id,
            team_name: format!("Team {team_id}"),
            school: None,
            participation_type: "OFFICIAL".into(),
            group_name: None,
            is_star: false,
            solved_count: i32::from(solved),
            penalty_minutes: 0,
            total_score_milli: if solved { 100_000 } else { 0 },
            last_solved_at: None,
            problems: vec![crate::features::scoreboard::ScoreboardCell {
                problem_id: 1,
                wrong_attempts: 0,
                solved,
                solved_at: None,
                penalty_minutes: 0,
                score_milli: if solved { 100_000 } else { 0 },
                first_blood: false,
            }],
        }
    }

    fn category_response_for_award_test(first_blood: bool) -> CategoryResponse {
        CategoryResponse {
            id: 1,
            contest_id: 1,
            code: "FB".into(),
            name: "First Blood".into(),
            display_order: 1,
            include_star: false,
            group_name: None,
            participation_type: None,
            first_blood,
            version: 0,
            rule_type: "FIXED_COUNT".into(),
            ratio: None,
            fixed_count: Some(100),
            rank_from: None,
            rank_to: None,
        }
    }

    fn fixed_category(code: &str, order: i32) -> CategoryRequest {
        CategoryRequest {
            code: code.into(),
            name: code.into(),
            display_order: order,
            include_star: false,
            group_name: None,
            participation_type: Some("official".into()),
            first_blood: false,
            rule: RuleRequest {
                rule_type: "fixed_count".into(),
                ratio: None,
                fixed_count: Some(1),
                rank_from: None,
                rank_to: None,
            },
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn awards_use_official_resolver_snapshot_and_freeze(pool: PgPool) {
        let user = sqlx::query_scalar::<_, i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('award-op','hash','Award Op','AWARD_OPERATOR') RETURNING id").fetch_one(&pool).await.expect("user");
        let contest = sqlx::query_scalar::<_, i64>("INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES('Award Contest','ENDED','PUBLIC',now()-interval '3 hours',now()-interval '2 hours',now()-interval '1 hour') RETURNING id").fetch_one(&pool).await.expect("contest");
        let mut rows = Vec::new();
        for rank in 1_u32..=2 {
            let team =
                sqlx::query_scalar::<_, i64>("INSERT INTO teams(name) VALUES($1) RETURNING id")
                    .bind(format!("Award Team {rank}"))
                    .fetch_one(&pool)
                    .await
                    .expect("team");
            rows.push(ScoreboardRow {
                rank,
                official_rank: Some(rank),
                team_id: team,
                team_name: format!("Award Team {rank}"),
                school: None,
                participation_type: "OFFICIAL".into(),
                group_name: None,
                is_star: false,
                solved_count: 3 - i32::try_from(rank).expect("rank"),
                penalty_minutes: i64::from(rank) * 60,
                total_score_milli: i64::from(3 - i32::try_from(rank).expect("rank")) * 100_000,
                last_solved_at: None,
                problems: Vec::new(),
            });
        }
        let board = ScoreboardResponse {
            contest_id: contest,
            variant: "ADMIN".into(),
            frozen: false,
            scoring_mode: "ICPC".into(),
            score_aggregation: "BEST".into(),
            generated_at: OffsetDateTime::now_utc(),
            problems: Vec::new(),
            rows,
        };
        let payload = serde_json::to_string(&board).expect("board");
        let sha = hex::encode(Sha256::digest(payload.as_bytes()));
        let final_id = sqlx::query_scalar::<_, i64>("INSERT INTO scoreboard_snapshots(contest_id,variant,version,frozen,generated_at,payload_json,payload_sha256,created_by,created_by_user_id) VALUES($1,'ADMIN',1,false,now(),$2,$3,'award-op',$4) RETURNING id").bind(contest).bind(&payload).bind(&sha).bind(user).fetch_one(&pool).await.expect("final");
        let public_id = sqlx::query_scalar::<_, i64>("INSERT INTO scoreboard_snapshots(contest_id,variant,version,frozen,generated_at,payload_json,payload_sha256,created_by,created_by_user_id) VALUES($1,'PUBLIC',1,true,now(),$2,$3,'award-op',$4) RETURNING id").bind(contest).bind(&payload).bind(&sha).bind(user).fetch_one(&pool).await.expect("public");
        let run = sqlx::query_scalar::<_, i64>("INSERT INTO resolver_runs(contest_id,official,status,current_step,total_steps,source_public_snapshot_id,source_final_snapshot_id,plan_sha256,created_by_user_id,started_at,completed_at) VALUES($1,true,'COMPLETED',0,0,$2,$3,$4,$5,now(),now()) RETURNING id").bind(contest).bind(public_id).bind(final_id).bind("a".repeat(64)).bind(user).fetch_one(&pool).await.expect("resolver");
        let actor = AuthUser {
            id: user,
            username: "award-op".into(),
            display_name: "Award Op".into(),
            user_type: UserType::AwardOperator,
            roles: vec!["AWARD_OPERATOR".into()],
            password_reset_required: false,
        };
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let service = AwardService::new(pool.clone());
        service
            .create_category(contest, fixed_category("GOLD", 1), &actor, ip)
            .await
            .expect("gold");
        let silver = service
            .create_category(contest, fixed_category("SILVER", 2), &actor, ip)
            .await
            .expect("silver");
        let generated = service.generate(contest, run, &actor, ip).await.expect("generate");
        assert_eq!(generated.final_scoreboard_snapshot_id, final_id);
        assert_eq!(generated.recipients.len(), 2);
        assert_eq!(generated.conflicts.len(), 1);
        assert_eq!(service.completed_resolver_runs(contest, &actor).await.expect("runs").len(), 1);
        assert_eq!(service.candidates(contest, &actor).await.expect("candidates").len(), 2);
        let with_manual = service
            .manual_add(
                contest,
                ManualRecipientRequest {
                    category_id: silver.id,
                    team_id: board.rows[1].team_id,
                    expected_set_version: generated.version,
                },
                &actor,
                ip,
            )
            .await
            .expect("memberless team certificate recipient");
        let first_team = board.rows[0].team_id;
        let member = sqlx::query_scalar::<_, i64>("INSERT INTO team_members(team_id,name,role_name) VALUES($1,'Alice, A','CAPTAIN') RETURNING id")
            .bind(first_team).fetch_one(&pool).await.expect("member");
        sqlx::query("INSERT INTO team_members(team_id,name,role_name) VALUES($1,'Bob','COACH')")
            .bind(first_team)
            .execute(&pool)
            .await
            .expect("coach");
        let frozen =
            service.freeze(contest, with_manual.version, true, &actor, ip).await.expect("freeze");
        assert_eq!(frozen.status, "FROZEN");
        sqlx::query("UPDATE team_members SET name='Changed after freeze' WHERE id=$1")
            .bind(member)
            .execute(&pool)
            .await
            .expect("edit member after freeze");
        let (_, certificates) =
            service.certificate_csv(contest, &actor, ip).await.expect("certificate export");
        assert!(certificates.starts_with("\u{feff}证书编号"));
        assert!(certificates.contains(&format!("XCPC-{contest}-R")));
        assert!(certificates.contains("\"Alice, A\""));
        assert!(certificates.contains(",Bob,COACH,"));
        assert!(certificates.contains(",TEAM,"));
        assert!(!certificates.contains("Changed after freeze"));
        let presentation = service.presentation(contest).await.expect("public presentation");
        assert_eq!(presentation.categories.len(), 2);
        assert_eq!(presentation.status, "WAITING");
        assert_eq!(presentation.current_category_id, presentation.categories[0].id);
        let silver_id = presentation.categories[1].id;
        let controlled = service
            .update_presentation(
                contest,
                PresentationRequest {
                    current_category_id: Some(silver_id),
                    status: "presenting".into(),
                    auto_rotate: true,
                    interval_seconds: 12,
                },
                &actor,
                ip,
            )
            .await
            .expect("control presentation");
        assert_eq!(controlled.current_category_id, silver_id);
        assert_eq!(controlled.status, "PRESENTING");
        assert!(controlled.auto_rotate);
        let script = service.host_script(contest).await.expect("default host script");
        assert_eq!(script.version, None);
        assert!(script.sections[1].current);
        let saved = service
            .save_host_script(
                contest,
                HostScriptRequest {
                    opening_text: "Welcome".into(),
                    closing_text: "Goodbye".into(),
                    sections: vec![HostScriptSectionRequest {
                        category_id: silver_id,
                        cue_text: "Silver teams, please come to the stage.".into(),
                    }],
                    expected_version: None,
                },
                &actor,
                ip,
            )
            .await
            .expect("save host script");
        assert_eq!(saved.version, Some(0));
        assert_eq!(saved.opening_text, "Welcome");
        assert_eq!(saved.sections[1].cue_text, "Silver teams, please come to the stage.");
        assert!(
            service
                .save_host_script(
                    contest,
                    HostScriptRequest {
                        opening_text: "stale".into(),
                        closing_text: String::new(),
                        sections: Vec::new(),
                        expected_version: None,
                    },
                    &actor,
                    ip,
                )
                .await
                .is_err()
        );
        let published = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM realtime_outbox WHERE contest_id=$1 AND event_type='AWARDS_UPDATED' AND scope='PUBLIC'")
            .bind(contest).fetch_one(&pool).await.expect("presentation event");
        assert_eq!(published, 1);
        assert!(
            service
                .create_category(contest, fixed_category("BRONZE", 3), &actor, ip)
                .await
                .is_err()
        );
        assert_eq!(
            service
                .freeze(contest, frozen.version, false, &actor, ip)
                .await
                .expect("unfreeze")
                .status,
            "DRAFT"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM award_certificate_rows WHERE contest_id=$1"
            )
            .bind(contest)
            .fetch_one(&pool)
            .await
            .expect("certificate rows"),
            0
        );
        assert!(service.certificate_csv(contest, &actor, ip).await.is_err());
    }
}
