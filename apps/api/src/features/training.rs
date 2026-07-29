use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, Transaction};
use std::collections::HashSet;
use utoipa::ToSchema;

use crate::{
    error::AppError,
    features::auth::{AuthContext, SuperAdminContext},
    pagination::PageResponse,
    state::AppState,
};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BankQuery {
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_size")]
    pub size: u32,
    pub tag: Option<String>,
    pub difficulty: Option<i16>,
}
const fn default_size() -> u32 {
    50
}

#[derive(Debug, Serialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BankProblem {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub statement: Option<String>,
    pub difficulty: Option<i16>,
    pub tags: serde_json::Value,
    pub published_at: time::OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TrainingSet {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub visibility: String,
    pub item_count: i64,
}

#[derive(Debug, Serialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TrainingItem {
    pub problem_id: i64,
    pub slug: String,
    pub title: String,
    pub position: i32,
    pub required: bool,
    pub difficulty: Option<i16>,
    pub tags: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrainingSetDetail {
    pub set_info: TrainingSet,
    pub items: Vec<TrainingItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetRequest {
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub visibility: String,
    #[serde(default)]
    pub items: Vec<SetItemRequest>,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetItemRequest {
    pub problem_id: i64,
    pub required: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicationRequest {
    pub visibility: String,
    pub difficulty: Option<i16>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressRequest {
    pub problem_id: i64,
    pub status: String,
    pub score: i32,
}

#[derive(Debug, Serialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Enrollment {
    pub id: i64,
    pub set_id: i64,
    pub team_id: Option<i64>,
    pub user_id: Option<i64>,
    pub status: String,
    pub started_at: time::OffsetDateTime,
    pub completed_at: Option<time::OffsetDateTime>,
}

fn validate_page(query: &BankQuery) -> Result<(i64, i64), AppError> {
    if !(1..=100).contains(&query.size) {
        return Err(AppError::validation("size", "must be between 1 and 100"));
    }
    let offset = i64::from(query.page)
        .checked_mul(i64::from(query.size))
        .ok_or_else(|| AppError::validation("page", "is too large"))?;
    Ok((i64::from(query.size), offset))
}
#[utoipa::path(get, path = "/api/public/problem-bank", operation_id = "listPublicProblemBank", tag = "training", params(("page" = Option<u32>, Query), ("size" = Option<u32>, Query), ("tag" = Option<String>, Query), ("difficulty" = Option<i16>, Query)), responses((status = 200, body = PageResponse<BankProblem>)))]
pub async fn list_bank(
    State(state): State<AppState>,
    query: Result<Query<BankQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<PageResponse<BankProblem>>, AppError> {
    let Query(query) = query.map_err(|_| AppError::validation("query", "invalid query"))?;
    let (size, offset) = validate_page(&query)?;
    let tag = query.tag.as_deref().map(str::trim).filter(|v| !v.is_empty());
    let total = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM problem_bank_entries WHERE visibility='PUBLIC' AND ($1::text IS NULL OR tags::jsonb ? $1) AND ($2::smallint IS NULL OR difficulty=$2)").bind(tag).bind(query.difficulty).fetch_one(state.database()).await.map_err(|e| AppError::internal("count public problem bank", e))?;
    let rows = sqlx::query_as::<_, BankProblem>("SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,b.tags::jsonb AS tags,b.published_at FROM problems p JOIN problem_bank_entries b ON b.problem_id=p.id LEFT JOIN problem_statements s ON s.problem_id=p.id AND s.lang_code=p.default_lang_code WHERE p.deleted_at IS NULL AND b.visibility='PUBLIC' AND ($1::text IS NULL OR b.tags::jsonb ? $1) AND ($2::smallint IS NULL OR b.difficulty=$2) ORDER BY b.published_at DESC,b.problem_id DESC LIMIT $3 OFFSET $4").bind(tag).bind(query.difficulty).bind(size).bind(offset).fetch_all(state.database()).await.map_err(|e| AppError::internal("list public problem bank", e))?;
    Ok(Json(PageResponse::new(rows, query.page, query.size, total)))
}

#[utoipa::path(get, path = "/api/public/problem-bank/{slug}", operation_id = "getPublicProblemBankProblem", tag = "training", params(("slug" = String, Path)), responses((status = 200, body = BankProblem), (status = 404, body = crate::error::ApiErrorBody)))]
pub async fn get_bank(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<BankProblem>, AppError> {
    let row = sqlx::query_as::<_, BankProblem>("SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,b.tags::jsonb AS tags,b.published_at FROM problems p JOIN problem_bank_entries b ON b.problem_id=p.id LEFT JOIN problem_statements s ON s.problem_id=p.id AND s.lang_code=p.default_lang_code WHERE p.slug=$1 AND p.deleted_at IS NULL AND b.visibility='PUBLIC'").bind(slug).fetch_optional(state.database()).await.map_err(|e| AppError::internal("get public problem bank problem", e))?.ok_or_else(|| AppError::not_found("PROBLEM_NOT_FOUND", "Problem is not public"))?;
    Ok(Json(row))
}

#[utoipa::path(put, path = "/api/admin/problems/{problem_id}/publication", operation_id = "updateProblemPublication", tag = "training", params(("problem_id" = i64, Path)), request_body = PublicationRequest, responses((status = 200, body = BankProblem), (status = 400, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_publication(
    context: SuperAdminContext,
    State(state): State<AppState>,
    Path(problem_id): Path<i64>,
    payload: Result<Json<PublicationRequest>, JsonRejection>,
) -> Result<Json<BankProblem>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid publication"))?;
    if problem_id <= 0 || !matches!(request.visibility.as_str(), "PRIVATE" | "PUBLIC") {
        return Err(AppError::validation("publication", "invalid problem or visibility"));
    }
    if request.difficulty.is_some_and(|v| !(0..=10).contains(&v)) {
        return Err(AppError::validation("difficulty", "must be between 0 and 10"));
    }
    if request.tags.len() > 32 || request.tags.iter().any(|v| v.trim().is_empty() || v.len() > 32) {
        return Err(AppError::validation("tags", "must contain at most 32 non-empty short tags"));
    }
    let tags = serde_json::to_string(
        &request.tags.iter().map(|v| v.trim().to_ascii_lowercase()).collect::<Vec<_>>(),
    )
    .map_err(|e| AppError::internal("encode tags", e))?;
    sqlx::query("INSERT INTO problem_bank_entries(problem_id,visibility,difficulty,tags,published_at,updated_at) VALUES($1,$2,$3,$4,CASE WHEN $2='PUBLIC' THEN coalesce((SELECT published_at FROM problem_bank_entries WHERE problem_id=$1),now()) ELSE NULL END,now()) ON CONFLICT(problem_id) DO UPDATE SET visibility=EXCLUDED.visibility,difficulty=EXCLUDED.difficulty,tags=EXCLUDED.tags,published_at=EXCLUDED.published_at,updated_at=now()")
        .bind(problem_id).bind(&request.visibility).bind(request.difficulty).bind(tags).execute(state.database()).await.map_err(|e| AppError::internal("update problem publication", e))?;
    let row = sqlx::query_as::<_, BankProblem>("SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,b.tags::jsonb AS tags,b.published_at FROM problems p JOIN problem_bank_entries b ON b.problem_id=p.id LEFT JOIN problem_statements s ON s.problem_id=p.id AND s.lang_code=p.default_lang_code WHERE p.id=$1").bind(problem_id).fetch_optional(state.database()).await.map_err(|e| AppError::internal("load problem publication", e))?.ok_or_else(|| AppError::not_found("PROBLEM_NOT_FOUND", "Problem not found"))?;
    let _ = context;
    Ok(Json(row))
}

async fn team_for_user(state: &AppState, user_id: i64) -> Result<Option<i64>, AppError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT team_id FROM team_accounts WHERE user_id=$1 ORDER BY team_id LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(state.database())
    .await
    .map_err(|e| AppError::internal("load training team", e))
}

#[utoipa::path(get, path = "/api/training/sets", operation_id = "listTrainingSets", tag = "training", responses((status = 200, body = [TrainingSet]), (status = 401, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_sets(
    _context: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Vec<TrainingSet>>, AppError> {
    Ok(Json(sqlx::query_as::<_, TrainingSet>("SELECT s.id,s.slug,s.title,s.description,s.visibility,count(i.problem_id)::bigint AS item_count FROM training_sets s LEFT JOIN training_set_items i ON i.set_id=s.id WHERE s.visibility='PUBLIC' GROUP BY s.id ORDER BY s.updated_at DESC,s.id DESC").fetch_all(state.database()).await.map_err(|e| AppError::internal("list training sets", e))?))
}

#[utoipa::path(get, path = "/api/training/sets/{set_id}", operation_id = "getTrainingSet", tag = "training", params(("set_id" = i64, Path)), responses((status = 200, body = TrainingSetDetail), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get_set(
    _context: AuthContext,
    State(state): State<AppState>,
    Path(set_id): Path<i64>,
) -> Result<Json<TrainingSetDetail>, AppError> {
    let set_info=sqlx::query_as::<_,TrainingSet>("SELECT s.id,s.slug,s.title,s.description,s.visibility,count(i.problem_id)::bigint AS item_count FROM training_sets s LEFT JOIN training_set_items i ON i.set_id=s.id WHERE s.id=$1 AND s.visibility='PUBLIC' GROUP BY s.id").bind(set_id).fetch_optional(state.database()).await.map_err(|e| AppError::internal("get training set", e))?.ok_or_else(|| AppError::not_found("TRAINING_SET_NOT_FOUND","Training set not found"))?;
    let items=sqlx::query_as::<_,TrainingItem>("SELECT i.problem_id,p.slug,p.title,i.position,i.required,b.difficulty,coalesce(b.tags,'[]')::jsonb AS tags FROM training_set_items i JOIN problems p ON p.id=i.problem_id LEFT JOIN problem_bank_entries b ON b.problem_id=p.id WHERE i.set_id=$1 ORDER BY i.position").bind(set_id).fetch_all(state.database()).await.map_err(|e| AppError::internal("list training items",e))?;
    Ok(Json(TrainingSetDetail { set_info, items }))
}

fn validate_set_request(request: &SetRequest) -> Result<String, AppError> {
    if request.slug.len() < 2
        || request.slug.len() > 96
        || !request.slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(AppError::validation(
            "slug",
            "must use 2-96 lowercase letters, digits, or hyphens",
        ));
    }
    if request.title.trim().is_empty()
        || request.title.len() > 255
        || request.description.len() > 10_000
    {
        return Err(AppError::validation("title", "title or description is invalid"));
    }
    let visibility =
        if request.visibility.is_empty() { "DRAFT" } else { request.visibility.as_str() };
    if !matches!(visibility, "DRAFT" | "PUBLIC" | "ARCHIVED") {
        return Err(AppError::validation("visibility", "must be DRAFT, PUBLIC, or ARCHIVED"));
    }
    if request.items.len() > 500 {
        return Err(AppError::validation("items", "must contain at most 500 problems"));
    }
    let mut seen = HashSet::new();
    for item in &request.items {
        if item.problem_id <= 0 || !seen.insert(item.problem_id) {
            return Err(AppError::validation("items", "problem IDs must be positive and unique"));
        }
    }
    Ok(visibility.to_owned())
}

async fn write_set(
    tx: &mut Transaction<'_, Postgres>,
    set_id: Option<i64>,
    request: &SetRequest,
    visibility: &str,
    user_id: i64,
) -> Result<i64, AppError> {
    let id = if let Some(id) = set_id {
        sqlx::query_scalar::<_, i64>("UPDATE training_sets SET slug=$2,title=$3,description=$4,visibility=$5,updated_at=now() WHERE id=$1 RETURNING id")
            .bind(id).bind(&request.slug).bind(request.title.trim()).bind(request.description.trim()).bind(visibility).fetch_optional(&mut **tx).await.map_err(|e| AppError::internal("update training set",e))?.ok_or_else(|| AppError::not_found("TRAINING_SET_NOT_FOUND","Training set not found"))?
    } else {
        sqlx::query_scalar::<_, i64>("INSERT INTO training_sets(slug,title,description,visibility,created_by_user_id) VALUES($1,$2,$3,$4,$5) RETURNING id")
            .bind(&request.slug).bind(request.title.trim()).bind(request.description.trim()).bind(visibility).bind(user_id).fetch_one(&mut **tx).await.map_err(|e| AppError::internal("create training set",e))?
    };
    sqlx::query("DELETE FROM training_set_items WHERE set_id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::internal("replace training items", e))?;
    for (index, item) in request.items.iter().enumerate() {
        sqlx::query("INSERT INTO training_set_items(set_id,problem_id,position,required) VALUES($1,$2,$3,$4)").bind(id).bind(item.problem_id).bind(i32::try_from(index+1).unwrap_or(i32::MAX)).bind(item.required).execute(&mut **tx).await.map_err(|e| AppError::internal("insert training item",e))?;
    }
    Ok(id)
}

async fn load_set_summary(state: &AppState, set_id: i64) -> Result<TrainingSet, AppError> {
    sqlx::query_as::<_, TrainingSet>("SELECT s.id,s.slug,s.title,s.description,s.visibility,count(i.problem_id)::bigint AS item_count FROM training_sets s LEFT JOIN training_set_items i ON i.set_id=s.id WHERE s.id=$1 GROUP BY s.id")
        .bind(set_id).fetch_optional(state.database()).await.map_err(|e| AppError::internal("load training set",e))?.ok_or_else(|| AppError::not_found("TRAINING_SET_NOT_FOUND","Training set not found"))
}

#[utoipa::path(post, path = "/api/admin/training/sets", operation_id = "createTrainingSet", tag = "training", request_body = SetRequest, responses((status = 201, body = TrainingSet)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create_set(
    context: SuperAdminContext,
    State(state): State<AppState>,
    payload: Result<Json<SetRequest>, JsonRejection>,
) -> Result<(axum::http::StatusCode, Json<TrainingSet>), AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid training set"))?;
    let visibility = validate_set_request(&request)?;
    let mut tx =
        state.database().begin().await.map_err(|e| AppError::internal("begin training set", e))?;
    let id = write_set(&mut tx, None, &request, &visibility, context.user().id).await?;
    tx.commit().await.map_err(|e| AppError::internal("commit training set", e))?;
    Ok((axum::http::StatusCode::CREATED, Json(load_set_summary(&state, id).await?)))
}

#[utoipa::path(put, path = "/api/admin/training/sets/{set_id}", operation_id = "updateTrainingSet", tag = "training", params(("set_id" = i64, Path)), request_body = SetRequest, responses((status = 200, body = TrainingSet)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_set(
    context: SuperAdminContext,
    State(state): State<AppState>,
    Path(set_id): Path<i64>,
    payload: Result<Json<SetRequest>, JsonRejection>,
) -> Result<Json<TrainingSet>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid training set"))?;
    let visibility = validate_set_request(&request)?;
    let mut tx =
        state.database().begin().await.map_err(|e| AppError::internal("begin training set", e))?;
    let id = write_set(&mut tx, Some(set_id), &request, &visibility, context.user().id).await?;
    tx.commit().await.map_err(|e| AppError::internal("commit training set", e))?;
    Ok(Json(load_set_summary(&state, id).await?))
}

#[utoipa::path(post, path = "/api/training/sets/{set_id}/enroll", operation_id = "enrollTrainingSet", tag = "training", params(("set_id" = i64, Path)), responses((status = 201, body = Enrollment), (status = 200, body = Enrollment)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn enroll(
    context: AuthContext,
    State(state): State<AppState>,
    Path(set_id): Path<i64>,
) -> Result<(axum::http::StatusCode, Json<Enrollment>), AppError> {
    let team_id = team_for_user(&state, context.user().id).await?;
    let row = if let Some(team_id) = team_id {
        sqlx::query_as::<_,Enrollment>("INSERT INTO training_enrollments(set_id,team_id) SELECT $1,$2 WHERE EXISTS(SELECT 1 FROM training_sets WHERE id=$1 AND visibility='PUBLIC') ON CONFLICT(set_id,team_id) DO UPDATE SET status='ACTIVE',updated_at=now() RETURNING id,set_id,team_id,user_id,status,started_at,completed_at").bind(set_id).bind(team_id).fetch_optional(state.database()).await
    } else {
        sqlx::query_as::<_,Enrollment>("INSERT INTO training_enrollments(set_id,user_id) SELECT $1,$2 WHERE EXISTS(SELECT 1 FROM training_sets WHERE id=$1 AND visibility='PUBLIC') ON CONFLICT(set_id,user_id) WHERE user_id IS NOT NULL DO UPDATE SET status='ACTIVE',updated_at=now() RETURNING id,set_id,team_id,user_id,status,started_at,completed_at").bind(set_id).bind(context.user().id).fetch_optional(state.database()).await
    }.map_err(|e| AppError::internal("enroll training set",e))?.ok_or_else(|| AppError::not_found("TRAINING_SET_NOT_FOUND","Training set not found"))?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

#[utoipa::path(put, path = "/api/training/enrollments/{enrollment_id}/progress", operation_id = "updateTrainingProgress", tag = "training", params(("enrollment_id" = i64, Path)), request_body = ProgressRequest, responses((status = 200, body = Enrollment), (status = 400, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn progress(
    context: AuthContext,
    State(state): State<AppState>,
    Path(enrollment_id): Path<i64>,
    payload: Result<Json<ProgressRequest>, JsonRejection>,
) -> Result<Json<Enrollment>, AppError> {
    let Json(request) = payload.map_err(|_| AppError::validation("request", "invalid progress"))?;
    if request.problem_id <= 0
        || !matches!(request.status.as_str(), "TODO" | "IN_PROGRESS" | "SOLVED")
        || !(0..=100).contains(&request.score)
    {
        return Err(AppError::validation("progress", "invalid status, problem, or score"));
    }
    let team_id = team_for_user(&state, context.user().id).await?;
    let owner_id = team_id.unwrap_or(context.user().id);
    let is_team = team_id.is_some();
    let result=sqlx::query_as::<_,Enrollment>("WITH e AS (SELECT * FROM training_enrollments WHERE id=$1 AND (($3 AND team_id=$2) OR (NOT $3 AND user_id=$2))), p AS (INSERT INTO training_progress(enrollment_id,problem_id,status,attempts,best_score,solved_at) SELECT $1,$4,$5,1,$6,CASE WHEN $5='SOLVED' THEN now() ELSE NULL END FROM e ON CONFLICT(enrollment_id,problem_id) DO UPDATE SET status=EXCLUDED.status,attempts=training_progress.attempts+1,best_score=GREATEST(training_progress.best_score,EXCLUDED.best_score),solved_at=coalesce(training_progress.solved_at,EXCLUDED.solved_at),updated_at=now() RETURNING enrollment_id) UPDATE training_enrollments SET status=CASE WHEN NOT EXISTS(SELECT 1 FROM training_set_items i WHERE i.set_id=e.set_id AND i.required AND NOT EXISTS(SELECT 1 FROM training_progress tp WHERE tp.enrollment_id=e.id AND tp.problem_id=i.problem_id AND tp.status='SOLVED')) THEN 'COMPLETED' ELSE 'ACTIVE' END,completed_at=CASE WHEN NOT EXISTS(SELECT 1 FROM training_set_items i WHERE i.set_id=e.set_id AND i.required AND NOT EXISTS(SELECT 1 FROM training_progress tp WHERE tp.enrollment_id=e.id AND tp.problem_id=i.problem_id AND tp.status='SOLVED')) THEN coalesce(training_enrollments.completed_at,now()) ELSE NULL END,updated_at=now() FROM e WHERE training_enrollments.id=e.id RETURNING training_enrollments.id,training_enrollments.set_id,training_enrollments.team_id,training_enrollments.user_id,training_enrollments.status,training_enrollments.started_at,training_enrollments.completed_at").bind(enrollment_id).bind(owner_id).bind(is_team).bind(request.problem_id).bind(&request.status).bind(request.score).fetch_optional(state.database()).await.map_err(|e|AppError::internal("update training progress",e))?.ok_or_else(||AppError::not_found("ENROLLMENT_NOT_FOUND","Enrollment not found"))?;
    Ok(Json(result))
}
