use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use sqlx::PgPool;

use crate::features::training::model::{
    BankProblem, BankProblemRow, BankQuery, ProblemPublication, PublicationRequest, TrainingItem,
    TrainingSet, validate_page,
};
use crate::{
    error::AppError, features::auth::SuperAdminContext, features::problems::render_safe_statement,
    pagination::PageResponse, state::AppState,
};

#[utoipa::path(get, path = "/api/public/problem-bank", operation_id = "listPublicProblemBank", tag = "training", params(("page" = Option<u32>, Query), ("size" = Option<u32>, Query), ("tag" = Option<String>, Query), ("difficulty" = Option<i16>, Query)), responses((status = 200, body = PageResponse<BankProblem>)))]
pub async fn list_bank(
    State(state): State<AppState>,
    query: Result<Query<BankQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<PageResponse<BankProblem>>, AppError> {
    let Query(query) = query.map_err(|_| AppError::validation("query", "invalid query"))?;
    let (size, offset) = validate_page(&query)?;
    let tag = query.tag.as_deref().map(str::trim).filter(|v| !v.is_empty());
    let total = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT count(*)
        FROM problem_bank_entries b
        JOIN problems p
            ON p.id=b.problem_id AND p.deleted_at IS NULL
        WHERE b.visibility='PUBLIC'
            AND ($1::text IS NULL OR b.tags::jsonb ? $1)
            AND ($2::smallint IS NULL OR b.difficulty=$2)
        "#,
    )
    .bind(tag)
    .bind(query.difficulty)
    .fetch_one(state.database())
    .await
    .map_err(|e| AppError::internal("count public problem bank", e))?;
    let rows = sqlx::query_as::<_, BankProblemRow>(
        r#"
        SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,
               b.tags::jsonb AS tags,b.published_at,p.languages
        FROM problems p
        JOIN problem_bank_entries b
            ON b.problem_id=p.id
        LEFT JOIN problem_statements s
            ON s.problem_id=p.id AND s.lang_code=p.default_lang_code
        WHERE p.deleted_at IS NULL
            AND b.visibility='PUBLIC'
            AND ($1::text IS NULL OR b.tags::jsonb ? $1)
            AND ($2::smallint IS NULL OR b.difficulty=$2)
        ORDER BY b.published_at DESC,b.problem_id DESC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(tag)
    .bind(query.difficulty)
    .bind(size)
    .bind(offset)
    .fetch_all(state.database())
    .await
    .map_err(|e| AppError::internal("list public problem bank", e))?;
    let mut problems =
        rows.into_iter().map(BankProblem::try_from).collect::<Result<Vec<_>, _>>()?;
    for problem in &mut problems {
        problem.statement =
            problem.statement.take().map(|statement| render_safe_statement(&statement));
    }
    Ok(Json(PageResponse::new(problems, query.page, query.size, total)))
}

#[utoipa::path(get, path = "/api/public/problem-bank/{slug}", operation_id = "getPublicProblemBankProblem", tag = "training", params(("slug" = String, Path)), responses((status = 200, body = BankProblem), (status = 404, body = crate::error::ApiErrorBody)))]
pub async fn get_bank(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<BankProblem>, AppError> {
    let row = sqlx::query_as::<_, BankProblemRow>(
        r#"
        SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,
               b.tags::jsonb AS tags,b.published_at,p.languages
        FROM problems p
        JOIN problem_bank_entries b
            ON b.problem_id=p.id
        LEFT JOIN problem_statements s
            ON s.problem_id=p.id AND s.lang_code=p.default_lang_code
        WHERE p.slug=$1
            AND p.deleted_at IS NULL
            AND b.visibility='PUBLIC'
        "#,
    )
    .bind(slug)
    .fetch_optional(state.database())
    .await
    .map_err(|e| AppError::internal("get public problem bank problem", e))?
    .ok_or_else(|| AppError::not_found("PROBLEM_NOT_FOUND", "Problem is not public"))?;
    let mut problem: BankProblem = row.try_into()?;
    problem.statement = problem.statement.take().map(|statement| render_safe_statement(&statement));
    Ok(Json(problem))
}

#[utoipa::path(put, path = "/api/admin/problems/{problem_id}/publication", operation_id = "updateProblemPublication", tag = "training", params(("problem_id" = i64, Path)), request_body = PublicationRequest, responses((status = 200, body = ProblemPublication), (status = 400, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_publication(
    _context: SuperAdminContext,
    State(state): State<AppState>,
    Path(problem_id): Path<i64>,
    payload: Result<Json<PublicationRequest>, JsonRejection>,
) -> Result<Json<ProblemPublication>, AppError> {
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
    sqlx::query(
        r#"
        INSERT INTO problem_bank_entries
            (problem_id, visibility, difficulty, tags, published_at, updated_at)
        VALUES($1, $2, $3, $4,
            CASE WHEN $2 = 'PUBLIC'
                THEN coalesce((SELECT published_at FROM problem_bank_entries WHERE problem_id = $1), now())
                ELSE NULL END,
            now())
        ON CONFLICT(problem_id) DO UPDATE SET
            visibility = EXCLUDED.visibility,
            difficulty = EXCLUDED.difficulty,
            tags = EXCLUDED.tags,
            published_at = EXCLUDED.published_at,
            updated_at = now()
        "#,
    )
        .bind(problem_id).bind(&request.visibility).bind(request.difficulty).bind(tags).execute(&mut *transaction).await.map_err(|e| AppError::internal("update problem publication", e))?;
    transaction
        .commit()
        .await
        .map_err(|e| AppError::internal("commit problem publication update", e))?;
    load_publication(&state, problem_id).await.map(Json)
}

#[utoipa::path(get, path = "/api/admin/problems/{problem_id}/publication", operation_id = "getProblemPublication", tag = "training", params(("problem_id" = i64, Path)), responses((status = 200, body = ProblemPublication), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn get_publication(
    _context: SuperAdminContext,
    State(state): State<AppState>,
    Path(problem_id): Path<i64>,
) -> Result<Json<ProblemPublication>, AppError> {
    if problem_id <= 0 {
        return Err(AppError::not_found("PROBLEM_NOT_FOUND", "Problem not found"));
    }
    load_publication(&state, problem_id).await.map(Json)
}

async fn load_publication(
    state: &AppState,
    problem_id: i64,
) -> Result<ProblemPublication, AppError> {
    let row = sqlx::query_as::<
        _,
        (Option<String>, Option<i16>, Option<serde_json::Value>, Option<time::OffsetDateTime>),
    >(
        "SELECT b.visibility,b.difficulty,b.tags::jsonb,b.published_at
         FROM problems p
         LEFT JOIN problem_bank_entries b ON b.problem_id=p.id
         WHERE p.id=$1 AND p.deleted_at IS NULL",
    )
    .bind(problem_id)
    .fetch_optional(state.database())
    .await
    .map_err(|e| AppError::internal("load problem publication", e))?
    .ok_or_else(|| AppError::not_found("PROBLEM_NOT_FOUND", "Problem not found"))?;
    let (visibility, difficulty, tags, published_at) = row;
    let tags = tags
        .map(serde_json::from_value::<Vec<String>>)
        .transpose()
        .map_err(|e| AppError::internal("decode problem publication tags", e))?
        .unwrap_or_default();
    Ok(ProblemPublication {
        visibility: visibility.unwrap_or_else(|| "PRIVATE".to_owned()),
        difficulty,
        tags,
        published_at,
    })
}

pub(super) async fn team_for_user(state: &AppState, user_id: i64) -> Result<Option<i64>, AppError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT team_id FROM team_accounts WHERE user_id=$1 ORDER BY team_id LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(state.database())
    .await
    .map_err(|e| AppError::internal("load training team", e))
}
pub(super) async fn load_public_training_sets(
    database: &PgPool,
) -> Result<Vec<TrainingSet>, sqlx::Error> {
    sqlx::query_as::<_, TrainingSet>(
        r#"
        SELECT s.id,s.slug,s.title,s.description,s.visibility,
               count(b.problem_id)::bigint AS item_count
        FROM training_sets s
        LEFT JOIN training_set_items i
            ON i.set_id=s.id
        LEFT JOIN problems p
            ON p.id=i.problem_id AND p.deleted_at IS NULL
        LEFT JOIN problem_bank_entries b
            ON b.problem_id=p.id AND b.visibility='PUBLIC'
        WHERE s.visibility='PUBLIC'
        GROUP BY s.id
        ORDER BY s.updated_at DESC,s.id DESC
        "#,
    )
    .fetch_all(database)
    .await
}

pub(super) async fn load_public_training_set(
    database: &PgPool,
    set_id: i64,
) -> Result<Option<TrainingSet>, sqlx::Error> {
    sqlx::query_as::<_, TrainingSet>(
        r#"
        SELECT s.id,s.slug,s.title,s.description,s.visibility,
               count(b.problem_id)::bigint AS item_count
        FROM training_sets s
        LEFT JOIN training_set_items i
            ON i.set_id=s.id
        LEFT JOIN problems p
            ON p.id=i.problem_id AND p.deleted_at IS NULL
        LEFT JOIN problem_bank_entries b
            ON b.problem_id=p.id AND b.visibility='PUBLIC'
        WHERE s.id=$1 AND s.visibility='PUBLIC'
        GROUP BY s.id
        "#,
    )
    .bind(set_id)
    .fetch_optional(database)
    .await
}

pub(super) async fn load_public_training_items(
    database: &PgPool,
    set_id: i64,
) -> Result<Vec<TrainingItem>, sqlx::Error> {
    sqlx::query_as::<_, TrainingItem>(
        r#"
        SELECT i.problem_id,p.slug,p.title,i.position,i.required,
               b.difficulty,b.tags::jsonb AS tags
        FROM training_set_items i
        JOIN problems p
            ON p.id=i.problem_id AND p.deleted_at IS NULL
        JOIN problem_bank_entries b
            ON b.problem_id=p.id AND b.visibility='PUBLIC'
        WHERE i.set_id=$1
        ORDER BY i.position
        "#,
    )
    .bind(set_id)
    .fetch_all(database)
    .await
}
