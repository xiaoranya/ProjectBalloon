use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder, Transaction};
use std::collections::HashSet;
use utoipa::ToSchema;

use crate::{
    error::AppError,
    features::auth::{AuthContext, SuperAdminContext},
    features::problems::render_safe_statement,
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
    pub published_at: Option<time::OffsetDateTime>,
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

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FavoriteRequest {
    pub favorite: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteResponse {
    problem_id: i64,
    favorite: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorialRequest {
    pub title: String,
    pub body: String,
    pub unlock_policy: String,
    pub published: bool,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct EditorialResponse {
    problem_id: i64,
    lang_code: String,
    title: String,
    body_html: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body_markdown: Option<String>,
    unlock_policy: String,
    unlocked: bool,
    updated_at: time::OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PracticeSettingsResponse {
    daily_submission_limit: i32,
    concurrent_judging_limit: i32,
    source_retention_days: i32,
    updated_at: time::OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PracticeSettingsRequest {
    daily_submission_limit: i32,
    concurrent_judging_limit: i32,
    source_retention_days: i32,
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
    let total = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM problem_bank_entries b JOIN problems p ON p.id=b.problem_id AND p.deleted_at IS NULL WHERE b.visibility='PUBLIC' AND ($1::text IS NULL OR b.tags::jsonb ? $1) AND ($2::smallint IS NULL OR b.difficulty=$2)").bind(tag).bind(query.difficulty).fetch_one(state.database()).await.map_err(|e| AppError::internal("count public problem bank", e))?;
    let mut rows = sqlx::query_as::<_, BankProblem>("SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,b.tags::jsonb AS tags,b.published_at FROM problems p JOIN problem_bank_entries b ON b.problem_id=p.id LEFT JOIN problem_statements s ON s.problem_id=p.id AND s.lang_code=p.default_lang_code WHERE p.deleted_at IS NULL AND b.visibility='PUBLIC' AND ($1::text IS NULL OR b.tags::jsonb ? $1) AND ($2::smallint IS NULL OR b.difficulty=$2) ORDER BY b.published_at DESC,b.problem_id DESC LIMIT $3 OFFSET $4").bind(tag).bind(query.difficulty).bind(size).bind(offset).fetch_all(state.database()).await.map_err(|e| AppError::internal("list public problem bank", e))?;
    for row in &mut rows {
        row.statement = row.statement.take().map(|statement| render_safe_statement(&statement));
    }
    Ok(Json(PageResponse::new(rows, query.page, query.size, total)))
}

#[utoipa::path(get, path = "/api/public/problem-bank/{slug}", operation_id = "getPublicProblemBankProblem", tag = "training", params(("slug" = String, Path)), responses((status = 200, body = BankProblem), (status = 404, body = crate::error::ApiErrorBody)))]
pub async fn get_bank(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<BankProblem>, AppError> {
    let mut row = sqlx::query_as::<_, BankProblem>("SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,b.tags::jsonb AS tags,b.published_at FROM problems p JOIN problem_bank_entries b ON b.problem_id=p.id LEFT JOIN problem_statements s ON s.problem_id=p.id AND s.lang_code=p.default_lang_code WHERE p.slug=$1 AND p.deleted_at IS NULL AND b.visibility='PUBLIC'").bind(slug).fetch_optional(state.database()).await.map_err(|e| AppError::internal("get public problem bank problem", e))?.ok_or_else(|| AppError::not_found("PROBLEM_NOT_FOUND", "Problem is not public"))?;
    row.statement = row.statement.take().map(|statement| render_safe_statement(&statement));
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
    let mut transaction = state
        .database()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin problem publication update", e))?;
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM problems WHERE id=$1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(problem_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|e| AppError::internal("lock problem for publication update", e))?
    .ok_or_else(|| AppError::not_found("PROBLEM_NOT_FOUND", "Problem not found"))?;
    sqlx::query("INSERT INTO problem_bank_entries(problem_id,visibility,difficulty,tags,published_at,updated_at) VALUES($1,$2,$3,$4,CASE WHEN $2='PUBLIC' THEN coalesce((SELECT published_at FROM problem_bank_entries WHERE problem_id=$1),now()) ELSE NULL END,now()) ON CONFLICT(problem_id) DO UPDATE SET visibility=EXCLUDED.visibility,difficulty=EXCLUDED.difficulty,tags=EXCLUDED.tags,published_at=EXCLUDED.published_at,updated_at=now()")
        .bind(problem_id).bind(&request.visibility).bind(request.difficulty).bind(tags).execute(&mut *transaction).await.map_err(|e| AppError::internal("update problem publication", e))?;
    let row = sqlx::query_as::<_, BankProblem>("SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,b.tags::jsonb AS tags,b.published_at FROM problems p JOIN problem_bank_entries b ON b.problem_id=p.id LEFT JOIN problem_statements s ON s.problem_id=p.id AND s.lang_code=p.default_lang_code WHERE p.id=$1 AND p.deleted_at IS NULL").bind(problem_id).fetch_one(&mut *transaction).await.map_err(|e| AppError::internal("load problem publication", e))?;
    transaction
        .commit()
        .await
        .map_err(|e| AppError::internal("commit problem publication update", e))?;
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
    context: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Vec<TrainingSet>>, AppError> {
    context.require_password_ready()?;
    Ok(Json(
        load_public_training_sets(state.database())
            .await
            .map_err(|e| AppError::internal("list training sets", e))?,
    ))
}

#[utoipa::path(get, path = "/api/training/sets/{set_id}", operation_id = "getTrainingSet", tag = "training", params(("set_id" = i64, Path)), responses((status = 200, body = TrainingSetDetail), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get_set(
    context: AuthContext,
    State(state): State<AppState>,
    Path(set_id): Path<i64>,
) -> Result<Json<TrainingSetDetail>, AppError> {
    context.require_password_ready()?;
    let set_info = load_public_training_set(state.database(), set_id)
        .await
        .map_err(|e| AppError::internal("get training set", e))?
        .ok_or_else(|| AppError::not_found("TRAINING_SET_NOT_FOUND", "Training set not found"))?;
    let items = load_public_training_items(state.database(), set_id)
        .await
        .map_err(|e| AppError::internal("list training items", e))?;
    Ok(Json(TrainingSetDetail { set_info, items }))
}

async fn load_public_training_sets(database: &PgPool) -> Result<Vec<TrainingSet>, sqlx::Error> {
    sqlx::query_as::<_, TrainingSet>(
        "SELECT s.id,s.slug,s.title,s.description,s.visibility,count(b.problem_id)::bigint AS item_count FROM training_sets s LEFT JOIN training_set_items i ON i.set_id=s.id LEFT JOIN problems p ON p.id=i.problem_id AND p.deleted_at IS NULL LEFT JOIN problem_bank_entries b ON b.problem_id=p.id AND b.visibility='PUBLIC' WHERE s.visibility='PUBLIC' GROUP BY s.id ORDER BY s.updated_at DESC,s.id DESC",
    )
    .fetch_all(database)
    .await
}

async fn load_public_training_set(
    database: &PgPool,
    set_id: i64,
) -> Result<Option<TrainingSet>, sqlx::Error> {
    sqlx::query_as::<_, TrainingSet>(
        "SELECT s.id,s.slug,s.title,s.description,s.visibility,count(b.problem_id)::bigint AS item_count FROM training_sets s LEFT JOIN training_set_items i ON i.set_id=s.id LEFT JOIN problems p ON p.id=i.problem_id AND p.deleted_at IS NULL LEFT JOIN problem_bank_entries b ON b.problem_id=p.id AND b.visibility='PUBLIC' WHERE s.id=$1 AND s.visibility='PUBLIC' GROUP BY s.id",
    )
    .bind(set_id)
    .fetch_optional(database)
    .await
}

async fn load_public_training_items(
    database: &PgPool,
    set_id: i64,
) -> Result<Vec<TrainingItem>, sqlx::Error> {
    sqlx::query_as::<_, TrainingItem>(
        "SELECT i.problem_id,p.slug,p.title,i.position,i.required,b.difficulty,b.tags::jsonb AS tags FROM training_set_items i JOIN problems p ON p.id=i.problem_id AND p.deleted_at IS NULL JOIN problem_bank_entries b ON b.problem_id=p.id AND b.visibility='PUBLIC' WHERE i.set_id=$1 ORDER BY i.position",
    )
    .bind(set_id)
    .fetch_all(database)
    .await
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
    if visibility == "PUBLIC" {
        let problem_ids = request.items.iter().map(|item| item.problem_id).collect::<Vec<_>>();
        let public_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM problems p JOIN problem_bank_entries b ON b.problem_id=p.id WHERE p.id=ANY($1) AND p.deleted_at IS NULL AND b.visibility='PUBLIC'",
        )
        .bind(&problem_ids)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::internal("validate public training items", e))?;
        if public_count != i64::try_from(problem_ids.len()).unwrap_or(i64::MAX) {
            return Err(AppError::validation(
                "items",
                "public training sets may only contain active public problems",
            ));
        }
    }
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
    if !request.items.is_empty() {
        let mut query = QueryBuilder::<Postgres>::new(
            "INSERT INTO training_set_items(set_id,problem_id,position,required) ",
        );
        query.push_values(request.items.iter().enumerate(), |mut bind, (index, item)| {
            bind.push_bind(id)
                .push_bind(item.problem_id)
                .push_bind(i32::try_from(index + 1).unwrap_or(i32::MAX))
                .push_bind(item.required);
        });
        query
            .build()
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::internal("insert training items", e))?;
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
    context.require_password_ready()?;
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
    context.require_password_ready()?;
    let Json(request) = payload.map_err(|_| AppError::validation("request", "invalid progress"))?;
    validate_progress_request(&request)?;
    let team_id = team_for_user(&state, context.user().id).await?;
    let owner_id = team_id.unwrap_or(context.user().id);
    let is_team = team_id.is_some();
    let result=sqlx::query_as::<_,Enrollment>("WITH e AS (SELECT e.* FROM training_enrollments e JOIN training_set_items i ON i.set_id=e.set_id AND i.problem_id=$4 WHERE e.id=$1 AND e.status IN ('ACTIVE','COMPLETED') AND (($3 AND e.team_id=$2) OR (NOT $3 AND e.user_id=$2))), p AS (INSERT INTO training_progress(enrollment_id,problem_id,status,attempts,best_score,solved_at) SELECT $1,$4,$5,0,0,NULL FROM e ON CONFLICT(enrollment_id,problem_id) DO UPDATE SET status=CASE WHEN training_progress.status='SOLVED' THEN 'SOLVED' ELSE EXCLUDED.status END,updated_at=now() RETURNING enrollment_id) UPDATE training_enrollments SET status=CASE WHEN NOT EXISTS(SELECT 1 FROM training_set_items i WHERE i.set_id=e.set_id AND i.required AND NOT EXISTS(SELECT 1 FROM training_progress tp WHERE tp.enrollment_id=e.id AND tp.problem_id=i.problem_id AND tp.status='SOLVED')) THEN 'COMPLETED' ELSE 'ACTIVE' END,completed_at=CASE WHEN NOT EXISTS(SELECT 1 FROM training_set_items i WHERE i.set_id=e.set_id AND i.required AND NOT EXISTS(SELECT 1 FROM training_progress tp WHERE tp.enrollment_id=e.id AND tp.problem_id=i.problem_id AND tp.status='SOLVED')) THEN coalesce(training_enrollments.completed_at,now()) ELSE NULL END,updated_at=now() FROM e WHERE training_enrollments.id=e.id RETURNING training_enrollments.id,training_enrollments.set_id,training_enrollments.team_id,training_enrollments.user_id,training_enrollments.status,training_enrollments.started_at,training_enrollments.completed_at").bind(enrollment_id).bind(owner_id).bind(is_team).bind(request.problem_id).bind(&request.status).fetch_optional(state.database()).await.map_err(|e|AppError::internal("update training progress",e))?.ok_or_else(||AppError::not_found("ENROLLMENT_NOT_FOUND","Enrollment not found"))?;
    Ok(Json(result))
}

fn validate_progress_request(request: &ProgressRequest) -> Result<(), AppError> {
    if request.problem_id <= 0
        || !matches!(request.status.as_str(), "TODO" | "IN_PROGRESS")
        || request.score != 0
    {
        return Err(AppError::validation(
            "progress",
            "client progress may only set TODO or IN_PROGRESS with a zero score",
        ));
    }
    Ok(())
}

#[utoipa::path(get, path = "/api/practice/favorites", operation_id = "listPracticeFavorites", tag = "practice", responses((status = 200, body = [BankProblem])), security(("session_cookie" = [])))]
pub async fn list_favorites(
    context: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Vec<BankProblem>>, AppError> {
    context.require_password_ready()?;
    let rows=sqlx::query_as::<_,BankProblem>("SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,b.tags::jsonb AS tags,b.published_at FROM practice_problem_favorites f JOIN problems p ON p.id=f.problem_id AND p.deleted_at IS NULL JOIN problem_bank_entries b ON b.problem_id=p.id AND b.visibility='PUBLIC' LEFT JOIN problem_statements s ON s.problem_id=p.id AND s.lang_code=p.default_lang_code WHERE f.user_id=$1 ORDER BY f.created_at DESC").bind(context.user().id).fetch_all(state.database()).await.map_err(|e|AppError::internal("list practice favorites",e))?;
    Ok(Json(rows))
}

#[utoipa::path(put, path = "/api/practice/problems/{problem_id}/favorite", operation_id = "setPracticeFavorite", tag = "practice", params(("problem_id" = i64, Path)), request_body = FavoriteRequest, responses((status = 200, body = FavoriteResponse)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn set_favorite(
    context: AuthContext,
    State(state): State<AppState>,
    Path(problem_id): Path<i64>,
    payload: Result<Json<FavoriteRequest>, JsonRejection>,
) -> Result<Json<FavoriteResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid favorite state"))?;
    if request.favorite {
        let changed=sqlx::query("INSERT INTO practice_problem_favorites(user_id,problem_id) SELECT $1,$2 WHERE EXISTS(SELECT 1 FROM problem_bank_entries b JOIN problems p ON p.id=b.problem_id AND p.deleted_at IS NULL WHERE b.problem_id=$2 AND b.visibility='PUBLIC') ON CONFLICT DO NOTHING").bind(context.user().id).bind(problem_id).execute(state.database()).await.map_err(|e|AppError::internal("favorite problem",e))?.rows_affected();
        if changed == 0 {
            let public=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM problem_bank_entries b JOIN problems p ON p.id=b.problem_id AND p.deleted_at IS NULL WHERE b.problem_id=$1 AND b.visibility='PUBLIC')").bind(problem_id).fetch_one(state.database()).await.map_err(|e|AppError::internal("check favorite problem",e))?;
            if !public {
                return Err(AppError::not_found("PROBLEM_NOT_FOUND", "Public problem not found"));
            }
        }
    } else {
        sqlx::query("DELETE FROM practice_problem_favorites WHERE user_id=$1 AND problem_id=$2")
            .bind(context.user().id)
            .bind(problem_id)
            .execute(state.database())
            .await
            .map_err(|e| AppError::internal("unfavorite problem", e))?;
    }
    Ok(Json(FavoriteResponse { problem_id, favorite: request.favorite }))
}

#[utoipa::path(get, path = "/api/practice/problems/{problem_id}/editorial", operation_id = "getPracticeEditorial", tag = "practice", params(("problem_id" = i64, Path), ("lang" = Option<String>, Query)), responses((status = 200, body = EditorialResponse), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get_editorial(
    context: AuthContext,
    State(state): State<AppState>,
    Path(problem_id): Path<i64>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<EditorialResponse>, AppError> {
    context.require_password_ready()?;
    let lang = query.get("lang").map_or("en", String::as_str);
    let row=sqlx::query_as::<_,(String,String,String,time::OffsetDateTime)>("SELECT editorial.title,editorial.body,editorial.unlock_policy,editorial.updated_at FROM problem_editorials editorial JOIN problems problem ON problem.id=editorial.problem_id AND problem.deleted_at IS NULL JOIN problem_bank_entries bank ON bank.problem_id=problem.id AND bank.visibility='PUBLIC' WHERE editorial.problem_id=$1 AND editorial.lang_code=$2 AND editorial.published").bind(problem_id).bind(lang).fetch_optional(state.database()).await.map_err(|e|AppError::internal("load practice editorial",e))?.ok_or_else(||AppError::not_found("EDITORIAL_NOT_FOUND","Editorial not found"))?;
    let progress = sqlx::query_as::<_, (i32, bool)>(
        "SELECT attempts,solved FROM practice_problem_progress WHERE user_id=$1 AND problem_id=$2",
    )
    .bind(context.user().id)
    .bind(problem_id)
    .fetch_optional(state.database())
    .await
    .map_err(|e| AppError::internal("check editorial unlock", e))?
    .unwrap_or((0, false));
    let unlocked = match row.2.as_str() {
        "ALWAYS" => true,
        "AFTER_ATTEMPT" => progress.0 > 0,
        "AFTER_ACCEPTED" => progress.1,
        _ => false,
    };
    if !unlocked {
        return Err(AppError::forbidden(
            "EDITORIAL_LOCKED",
            "Editorial unlock condition is not met",
        ));
    }
    Ok(Json(EditorialResponse {
        problem_id,
        lang_code: lang.to_owned(),
        title: row.0,
        body_html: crate::features::problems::render_safe_statement(&row.1),
        body_markdown: None,
        unlock_policy: row.2,
        unlocked,
        updated_at: row.3,
    }))
}

#[utoipa::path(put, path = "/api/admin/problems/{problem_id}/editorials/{lang_code}", operation_id = "upsertProblemEditorial", tag = "practice", params(("problem_id" = i64, Path), ("lang_code" = String, Path)), request_body = EditorialRequest, responses((status = 200, body = EditorialResponse)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn upsert_editorial(
    context: SuperAdminContext,
    State(state): State<AppState>,
    Path((problem_id, lang_code)): Path<(i64, String)>,
    payload: Result<Json<EditorialRequest>, JsonRejection>,
) -> Result<Json<EditorialResponse>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid editorial"))?;
    if problem_id <= 0
        || lang_code.trim().is_empty()
        || lang_code.len() > 8
        || request.title.trim().is_empty()
        || request.title.len() > 255
        || request.body.trim().is_empty()
        || request.body.len() > 1024 * 1024
        || !matches!(request.unlock_policy.as_str(), "ALWAYS" | "AFTER_ATTEMPT" | "AFTER_ACCEPTED")
    {
        return Err(AppError::validation(
            "editorial",
            "invalid language, content, or unlock policy",
        ));
    }
    let updated=sqlx::query_as::<_,(time::OffsetDateTime,)>("INSERT INTO problem_editorials(problem_id,lang_code,title,body,unlock_policy,published,updated_by_user_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(problem_id,lang_code) DO UPDATE SET title=EXCLUDED.title,body=EXCLUDED.body,unlock_policy=EXCLUDED.unlock_policy,published=EXCLUDED.published,updated_by_user_id=EXCLUDED.updated_by_user_id,updated_at=now() RETURNING updated_at").bind(problem_id).bind(lang_code.trim()).bind(request.title.trim()).bind(&request.body).bind(&request.unlock_policy).bind(request.published).bind(context.user().id).fetch_one(state.database()).await.map_err(|e|AppError::internal("save problem editorial",e))?.0;
    Ok(Json(EditorialResponse {
        problem_id,
        lang_code: lang_code.trim().to_owned(),
        title: request.title.trim().to_owned(),
        body_html: crate::features::problems::render_safe_statement(&request.body),
        body_markdown: Some(request.body),
        unlock_policy: request.unlock_policy,
        unlocked: true,
        updated_at: updated,
    }))
}

#[utoipa::path(get, path = "/api/admin/problems/{problem_id}/editorials/{lang_code}", operation_id = "getAdminProblemEditorial", tag = "practice", params(("problem_id" = i64, Path), ("lang_code" = String, Path)), responses((status = 200, body = EditorialResponse), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get_admin_editorial(
    _context: SuperAdminContext,
    State(state): State<AppState>,
    Path((problem_id, lang_code)): Path<(i64, String)>,
) -> Result<Json<EditorialResponse>, AppError> {
    let row = sqlx::query_as::<_, (String, String, String, bool, time::OffsetDateTime)>(
        "SELECT title,body,unlock_policy,published,updated_at FROM problem_editorials WHERE problem_id=$1 AND lang_code=$2",
    )
    .bind(problem_id)
    .bind(lang_code.trim())
    .fetch_optional(state.database())
    .await
    .map_err(|e| AppError::internal("load admin problem editorial", e))?
    .ok_or_else(|| AppError::not_found("EDITORIAL_NOT_FOUND", "Editorial not found"))?;
    Ok(Json(EditorialResponse {
        problem_id,
        lang_code: lang_code.trim().to_owned(),
        title: row.0,
        body_html: crate::features::problems::render_safe_statement(&row.1),
        body_markdown: Some(row.1),
        unlock_policy: row.2,
        unlocked: row.3,
        updated_at: row.4,
    }))
}

#[utoipa::path(get, path = "/api/admin/practice/settings", operation_id = "getPracticeSettings", tag = "practice", responses((status = 200, body = PracticeSettingsResponse)), security(("session_cookie" = [])))]
pub async fn get_practice_settings(
    _context: SuperAdminContext,
    State(state): State<AppState>,
) -> Result<Json<PracticeSettingsResponse>, AppError> {
    let settings = sqlx::query_as::<_, PracticeSettingsResponse>(
        "SELECT daily_submission_limit,concurrent_judging_limit,source_retention_days,updated_at FROM practice_platform_settings WHERE singleton=true",
    )
    .fetch_one(state.database())
    .await
    .map_err(|e| AppError::internal("load practice settings", e))?;
    Ok(Json(settings))
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::{
        BankProblem, BankQuery, ProgressRequest, SetItemRequest, SetRequest,
        load_public_training_items, load_public_training_set, load_public_training_sets,
        validate_page, validate_progress_request, validate_set_request, write_set,
    };

    #[test]
    fn training_queries_and_sets_reject_invalid_bounds_and_duplicate_items() {
        assert_eq!(
            validate_page(&BankQuery { page: 2, size: 25, tag: None, difficulty: None })
                .expect("valid page"),
            (25, 50)
        );
        assert!(
            validate_page(&BankQuery { page: 2, size: 0, tag: None, difficulty: None }).is_err()
        );
        assert!(
            validate_page(&BankQuery { page: 2, size: 101, tag: None, difficulty: None }).is_err()
        );

        let mut valid = SetRequest {
            slug: "graphs-101".into(),
            title: "Graphs".into(),
            description: String::new(),
            visibility: String::new(),
            items: vec![SetItemRequest { problem_id: 7, required: true }],
        };
        assert_eq!(validate_set_request(&valid).expect("default visibility"), "DRAFT");
        valid.visibility = "PUBLIC".into();
        assert_eq!(validate_set_request(&valid).expect("public set"), "PUBLIC");

        valid.items.push(SetItemRequest { problem_id: 7, required: false });
        assert!(validate_set_request(&valid).is_err());
        valid.items[1].problem_id = 0;
        assert!(validate_set_request(&valid).is_err());
    }

    #[test]
    fn client_training_progress_cannot_claim_a_solution_or_score() {
        let valid = ProgressRequest { problem_id: 7, status: "IN_PROGRESS".into(), score: 0 };
        assert!(validate_progress_request(&valid).is_ok());
        assert!(
            validate_progress_request(&ProgressRequest { status: "SOLVED".into(), ..valid })
                .is_err()
        );
        assert!(
            validate_progress_request(&ProgressRequest {
                problem_id: 7,
                status: "IN_PROGRESS".into(),
                score: 100,
            })
            .is_err()
        );
        assert!(
            validate_progress_request(&ProgressRequest {
                problem_id: 0,
                status: "IN_PROGRESS".into(),
                score: 0,
            })
            .is_err()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn public_training_sets_only_expose_active_public_problems(pool: PgPool) {
        let user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('training-admin', 'test-hash', 'Training Admin', 'SUPER_ADMIN', true, false) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert training admin");
        let public_problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('training-public', 'Public') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert public problem");
        let private_problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('training-private', 'Private') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert private problem");
        let deleted_problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('training-deleted', 'Deleted') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert deleted problem");
        sqlx::query(
            "INSERT INTO problem_bank_entries (problem_id, visibility, tags, published_at) VALUES ($1, 'PUBLIC', '[]', now()), ($2, 'PUBLIC', '[]', now()), ($3, 'PUBLIC', '[]', now())",
        )
        .bind(public_problem_id)
        .bind(private_problem_id)
        .bind(deleted_problem_id)
        .execute(&pool)
        .await
        .expect("insert problem bank entries");

        let request = SetRequest {
            slug: "mixed-training".into(),
            title: "Mixed Training".into(),
            description: String::new(),
            visibility: "PUBLIC".into(),
            items: vec![
                SetItemRequest { problem_id: public_problem_id, required: true },
                SetItemRequest { problem_id: private_problem_id, required: false },
                SetItemRequest { problem_id: deleted_problem_id, required: false },
            ],
        };
        let mut tx = pool.begin().await.expect("begin training set transaction");
        let set_id = write_set(&mut tx, None, &request, "PUBLIC", user_id)
            .await
            .expect("create initially public training set");
        tx.commit().await.expect("commit training set");

        sqlx::query("UPDATE problem_bank_entries SET visibility = 'PRIVATE' WHERE problem_id = $1")
            .bind(private_problem_id)
            .execute(&pool)
            .await
            .expect("make problem private");
        sqlx::query("UPDATE problems SET deleted_at = now() WHERE id = $1")
            .bind(deleted_problem_id)
            .execute(&pool)
            .await
            .expect("soft-delete problem");

        let sets = load_public_training_sets(&pool).await.expect("list public training sets");
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].item_count, 1);
        let summary = load_public_training_set(&pool, set_id)
            .await
            .expect("load public training set")
            .expect("public training set exists");
        assert_eq!(summary.item_count, 1);
        let items =
            load_public_training_items(&pool, set_id).await.expect("load public training items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].problem_id, public_problem_id);

        let mut tx = pool.begin().await.expect("begin invalid training set update");
        assert!(write_set(&mut tx, Some(set_id), &request, "PUBLIC", user_id).await.is_err());
        tx.rollback().await.expect("rollback invalid training set update");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn private_problem_publication_response_allows_missing_published_at(pool: PgPool) {
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('training-private-response', 'Private response') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert private problem");
        sqlx::query(
            "INSERT INTO problem_bank_entries (problem_id, visibility, tags, published_at) VALUES ($1, 'PRIVATE', '[]', NULL)",
        )
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("insert private publication");

        let row = sqlx::query_as::<_, BankProblem>(
            "SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,b.tags::jsonb AS tags,b.published_at FROM problems p JOIN problem_bank_entries b ON b.problem_id=p.id LEFT JOIN problem_statements s ON s.problem_id=p.id AND s.lang_code=p.default_lang_code WHERE p.id=$1",
        )
        .bind(problem_id)
        .fetch_one(&pool)
        .await
        .expect("load private publication response");

        assert_eq!(row.published_at, None);
    }
}

#[utoipa::path(put, path = "/api/admin/practice/settings", operation_id = "updatePracticeSettings", tag = "practice", request_body = PracticeSettingsRequest, responses((status = 200, body = PracticeSettingsResponse), (status = 400, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_practice_settings(
    context: SuperAdminContext,
    State(state): State<AppState>,
    payload: Result<Json<PracticeSettingsRequest>, JsonRejection>,
) -> Result<Json<PracticeSettingsResponse>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid practice settings"))?;
    if !(1..=10_000).contains(&request.daily_submission_limit) {
        return Err(AppError::validation("dailySubmissionLimit", "must be between 1 and 10000"));
    }
    if !(1..=20).contains(&request.concurrent_judging_limit) {
        return Err(AppError::validation("concurrentJudgingLimit", "must be between 1 and 20"));
    }
    if !(1..=3_650).contains(&request.source_retention_days) {
        return Err(AppError::validation("sourceRetentionDays", "must be between 1 and 3650"));
    }
    let settings = sqlx::query_as::<_, PracticeSettingsResponse>(
        "UPDATE practice_platform_settings SET daily_submission_limit=$1,concurrent_judging_limit=$2,source_retention_days=$3,updated_by_user_id=$4,updated_at=now() WHERE singleton=true RETURNING daily_submission_limit,concurrent_judging_limit,source_retention_days,updated_at",
    )
    .bind(request.daily_submission_limit)
    .bind(request.concurrent_judging_limit)
    .bind(request.source_retention_days)
    .bind(context.user().id)
    .fetch_one(state.database())
    .await
    .map_err(|e| AppError::internal("update practice settings", e))?;
    Ok(Json(settings))
}
