use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
};
use sqlx::{Postgres, QueryBuilder, Transaction};
use std::collections::HashSet;

use crate::features::training::bank::{
    load_public_training_items, load_public_training_set, load_public_training_sets, team_for_user,
};
use crate::features::training::model::{
    Enrollment, ProgressRequest, SetRequest, TrainingSet, TrainingSetDetail,
};
use crate::{
    error::AppError,
    features::auth::{AuthContext, SuperAdminContext},
    state::AppState,
};

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
pub(super) fn validate_set_request(request: &SetRequest) -> Result<String, AppError> {
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

pub(super) async fn write_set(
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
        sqlx::query_as::<_, Enrollment>(
            r#"
            INSERT INTO training_enrollments(set_id,team_id)
            SELECT $1,$2
            WHERE EXISTS(SELECT 1 FROM training_sets WHERE id=$1 AND visibility='PUBLIC')
            ON CONFLICT(set_id,team_id) DO UPDATE
                SET status='ACTIVE',updated_at=now()
            RETURNING id,set_id,team_id,user_id,status,started_at,completed_at
            "#,
        )
        .bind(set_id)
        .bind(team_id)
        .fetch_optional(state.database())
        .await
    } else {
        sqlx::query_as::<_, Enrollment>(
            r#"
            INSERT INTO training_enrollments(set_id,user_id)
            SELECT $1,$2
            WHERE EXISTS(SELECT 1 FROM training_sets WHERE id=$1 AND visibility='PUBLIC')
            ON CONFLICT(set_id,user_id) WHERE user_id IS NOT NULL DO UPDATE
                SET status='ACTIVE',updated_at=now()
            RETURNING id,set_id,team_id,user_id,status,started_at,completed_at
            "#,
        )
        .bind(set_id)
        .bind(context.user().id)
        .fetch_optional(state.database())
        .await
    }
    .map_err(|e| AppError::internal("enroll training set", e))?
    .ok_or_else(|| AppError::not_found("TRAINING_SET_NOT_FOUND", "Training set not found"))?;
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
    let result = sqlx::query_as::<_, Enrollment>(
        r#"
        WITH e AS (
            SELECT e.*
            FROM training_enrollments e
            JOIN training_set_items i
                ON i.set_id=e.set_id AND i.problem_id=$4
            WHERE e.id=$1
                AND e.status IN ('ACTIVE','COMPLETED')
                AND (($3 AND e.team_id=$2) OR (NOT $3 AND e.user_id=$2))
        ),
        p AS (
            INSERT INTO training_progress
                (enrollment_id,problem_id,status,attempts,best_score,solved_at)
            SELECT $1,$4,$5,0,0,NULL
            FROM e
            ON CONFLICT(enrollment_id,problem_id) DO UPDATE
                SET status=CASE
                        WHEN training_progress.status='SOLVED' THEN 'SOLVED'
                        ELSE EXCLUDED.status
                    END,
                    updated_at=now()
            RETURNING enrollment_id
        )
        UPDATE training_enrollments
        SET status=CASE
                WHEN NOT EXISTS(
                    SELECT 1 FROM training_set_items i
                    WHERE i.set_id=e.set_id AND i.required
                        AND NOT EXISTS(
                            SELECT 1 FROM training_progress tp
                            WHERE tp.enrollment_id=e.id
                                AND tp.problem_id=i.problem_id
                                AND tp.status='SOLVED'
                        )
                ) THEN 'COMPLETED'
                ELSE 'ACTIVE'
            END,
            completed_at=CASE
                WHEN NOT EXISTS(
                    SELECT 1 FROM training_set_items i
                    WHERE i.set_id=e.set_id AND i.required
                        AND NOT EXISTS(
                            SELECT 1 FROM training_progress tp
                            WHERE tp.enrollment_id=e.id
                                AND tp.problem_id=i.problem_id
                                AND tp.status='SOLVED'
                        )
                ) THEN coalesce(training_enrollments.completed_at,now())
                ELSE NULL
            END,
            updated_at=now()
        FROM e
        WHERE training_enrollments.id=e.id
        RETURNING training_enrollments.id,training_enrollments.set_id,
            training_enrollments.team_id,training_enrollments.user_id,
            training_enrollments.status,training_enrollments.started_at,
            training_enrollments.completed_at
        "#,
    )
    .bind(enrollment_id)
    .bind(owner_id)
    .bind(is_team)
    .bind(request.problem_id)
    .bind(&request.status)
    .fetch_optional(state.database())
    .await
    .map_err(|e| AppError::internal("update training progress", e))?
    .ok_or_else(|| AppError::not_found("ENROLLMENT_NOT_FOUND", "Enrollment not found"))?;
    Ok(Json(result))
}

pub(super) fn validate_progress_request(request: &ProgressRequest) -> Result<(), AppError> {
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
