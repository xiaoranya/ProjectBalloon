use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};

use super::model::{
    BankProblem, BankProblemRow, EditorialRequest, EditorialResponse, FavoriteRequest,
    FavoriteResponse, PracticeSettingsRequest, PracticeSettingsResponse,
};
use crate::{
    error::AppError,
    features::auth::{AuthContext, SuperAdminContext},
    features::problems::render_safe_statement,
    state::AppState,
};

#[utoipa::path(get, path = "/api/practice/favorites", operation_id = "listPracticeFavorites", tag = "practice", responses((status = 200, body = [BankProblem])), security(("session_cookie" = [])))]
pub async fn list_favorites(
    context: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Vec<BankProblem>>, AppError> {
    context.require_password_ready()?;
    let rows=sqlx::query_as::<_,BankProblemRow>("SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,b.tags::jsonb AS tags,b.published_at,p.languages FROM practice_problem_favorites f JOIN problems p ON p.id=f.problem_id AND p.deleted_at IS NULL JOIN problem_bank_entries b ON b.problem_id=p.id AND b.visibility='PUBLIC' LEFT JOIN problem_statements s ON s.problem_id=p.id AND s.lang_code=p.default_lang_code WHERE f.user_id=$1 ORDER BY f.created_at DESC").bind(context.user().id).fetch_all(state.database()).await.map_err(|e|AppError::internal("list practice favorites",e))?;
    let problems = rows.into_iter().map(BankProblem::try_from).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(problems))
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
        body_html: render_safe_statement(&row.1),
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
        body_html: render_safe_statement(&request.body),
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
        body_html: render_safe_statement(&row.1),
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
