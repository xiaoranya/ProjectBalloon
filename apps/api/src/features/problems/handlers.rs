use std::net::SocketAddr;

use axum::{
    Json,
    body::Body,
    extract::{ConnectInfo, Multipart, Path, Query, State, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};

use crate::{
    error::AppError,
    features::auth::{AuthContext, ContestManagerContext, SuperAdminContext},
    pagination::PageResponse,
    state::AppState,
};

use crate::features::problems::model::{
    ActivateTestdataVersionRequest, AttachmentKind, AttachmentUploadRequest, CreateProblemRequest,
    InteractorUploadRequest, ProblemAttachmentResponse, ProblemListQuery, ProblemResponse,
    ProblemStatementResponse, ProblemTestdataResponse, ProblemTestdataVersionResponse,
    TestdataUploadRequest, UpdateProblemRequest, UpsertStatementRequest,
    validate_attachment_filename, validate_lang_code_field,
};

#[utoipa::path(
    get,
    path = "/api/problems",
    operation_id = "listProblems",
    tag = "problems",
    params(
        ("page" = Option<u32>, Query),
        ("size" = Option<u32>, Query),
        ("contestId" = Option<i64>, Query, description = "Required contest scope for contest managers")
    ),
    responses(
        (status = 200, description = "Problem catalog", body = PageResponse<ProblemResponse>),
        (status = 400, description = "Invalid pagination", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Super administrator or scoped contest manager access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or outside management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn list(
    context: ContestManagerContext,
    State(state): State<AppState>,
    query: Result<Query<ProblemListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<PageResponse<ProblemResponse>>, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "must contain valid pagination"))?;
    Ok(Json(state.problems().list(query, context.user()).await?))
}

#[utoipa::path(
    get,
    path = "/api/problems/{problem_id}",
    operation_id = "getProblem",
    tag = "problems",
    params(("problem_id" = i64, Path)),
    responses(
        (status = 200, description = "Problem", body = ProblemResponse),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem was not found or outside management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn get(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(problem_id): Path<i64>,
) -> Result<Json<ProblemResponse>, AppError> {
    Ok(Json(state.problems().get(problem_id, context.user()).await?))
}

#[utoipa::path(
    get,
    path = "/api/problems/{problem_id}/attachments",
    operation_id = "listProblemAttachments",
    tag = "problems",
    params(("problem_id" = i64, Path)),
    responses(
        (status = 200, description = "Problem attachments", body = [ProblemAttachmentResponse]),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem was not found or outside management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn list_attachments(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(problem_id): Path<i64>,
) -> Result<Json<Vec<ProblemAttachmentResponse>>, AppError> {
    Ok(Json(state.problems().list_attachments(problem_id, context.user()).await?))
}

#[utoipa::path(
    post,
    path = "/api/problems",
    operation_id = "createProblem",
    tag = "problems",
    request_body = CreateProblemRequest,
    responses(
        (status = 201, description = "Problem created", body = ProblemResponse),
        (status = 400, description = "Invalid problem configuration", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Super administrator, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 409, description = "Problem slug conflict", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn create(
    context: SuperAdminContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    payload: Result<Json<CreateProblemRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ProblemResponse>), AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid problem object"))?;
    let problem =
        state.problems().create(request.validate()?, context.user().id, peer.ip()).await?;
    Ok((StatusCode::CREATED, Json(problem)))
}

#[utoipa::path(
    patch,
    path = "/api/problems/{problem_id}",
    operation_id = "updateProblem",
    tag = "problems",
    params(("problem_id" = i64, Path)),
    request_body = UpdateProblemRequest,
    responses(
        (status = 200, description = "Problem updated", body = ProblemResponse),
        (status = 400, description = "Invalid update", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem was not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Problem slug, optimistic version, or frozen configuration conflict", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn update(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(problem_id): Path<i64>,
    payload: Result<Json<UpdateProblemRequest>, JsonRejection>,
) -> Result<Json<ProblemResponse>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid problem update"))?;
    Ok(Json(
        state.problems().update(problem_id, request.validate()?, context.user(), peer.ip()).await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/problems/{problem_id}",
    operation_id = "deleteProblem",
    tag = "problems",
    params(("problem_id" = i64, Path)),
    responses(
        (status = 204, description = "Problem deleted"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Super administrator, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem was not found", body = crate::error::ApiErrorBody),
        (status = 409, description = "Problem is referenced by a contest", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn delete(
    context: SuperAdminContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(problem_id): Path<i64>,
) -> Result<StatusCode, AppError> {
    state.problems().delete(problem_id, context.user().id, peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/problems/{problem_id}/statements",
    operation_id = "listProblemStatements",
    tag = "problems",
    params(("problem_id" = i64, Path)),
    responses(
        (status = 200, description = "Problem statements", body = [ProblemStatementResponse]),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem was not found or outside management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn list_statements(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(problem_id): Path<i64>,
) -> Result<Json<Vec<ProblemStatementResponse>>, AppError> {
    Ok(Json(state.problems().list_statements(problem_id, context.user()).await?))
}

#[utoipa::path(
    put,
    path = "/api/problems/{problem_id}/statements/{lang_code}",
    operation_id = "upsertProblemStatement",
    tag = "problems",
    params(("problem_id" = i64, Path), ("lang_code" = String, Path)),
    request_body = UpsertStatementRequest,
    responses(
        (status = 200, description = "Problem statement saved", body = ProblemStatementResponse),
        (status = 400, description = "Invalid language or statement", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem was not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Problem configuration is frozen", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn upsert_statement(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((problem_id, lang_code)): Path<(i64, String)>,
    payload: Result<Json<UpsertStatementRequest>, JsonRejection>,
) -> Result<Json<ProblemStatementResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid problem statement"))?;
    Ok(Json(
        state
            .problems()
            .upsert_statement(problem_id, request.validate(lang_code)?, context.user(), peer.ip())
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/problems/{problem_id}/statements/{lang_code}",
    operation_id = "deleteProblemStatement",
    tag = "problems",
    params(("problem_id" = i64, Path), ("lang_code" = String, Path)),
    responses(
        (status = 204, description = "Problem statement deleted"),
        (status = 400, description = "Invalid language", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem or statement was not found", body = crate::error::ApiErrorBody),
        (status = 409, description = "Problem configuration is frozen", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn delete_statement(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((problem_id, lang_code)): Path<(i64, String)>,
) -> Result<StatusCode, AppError> {
    let lang_code = validate_lang_code_field("langCode", lang_code)?;
    state.problems().delete_statement(problem_id, lang_code, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/problems/{problem_id}/attachments",
    operation_id = "uploadProblemAttachment",
    tag = "problems",
    params(("problem_id" = i64, Path)),
    request_body(content = inline(AttachmentUploadRequest), content_type = "multipart/form-data"),
    responses(
        (status = 201, description = "Problem attachment uploaded", body = ProblemAttachmentResponse),
        (status = 400, description = "Invalid multipart attachment", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem was not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 503, description = "Object storage is unavailable", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn upload_attachment(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(problem_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<ProblemAttachmentResponse>), AppError> {
    let mut kind = None;
    let mut upload = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::validation("request", "must be valid multipart data"))?
    {
        match field.name() {
            Some("kind") if kind.is_none() => {
                let value = field
                    .text()
                    .await
                    .map_err(|_| AppError::validation("kind", "must be valid text"))?;
                kind = Some(AttachmentKind::parse(&value)?);
            }
            Some("file") if upload.is_none() => {
                let filename = validate_attachment_filename(
                    field
                        .file_name()
                        .ok_or_else(|| AppError::validation("file", "must include a filename"))?
                        .to_owned(),
                )?;
                let content_type = field.content_type().map(str::to_ascii_lowercase);
                validate_attachment_content_type(content_type.as_deref())?;
                let content = field
                    .bytes()
                    .await
                    .map_err(|_| AppError::validation("file", "could not be read"))?;
                upload = Some((filename, content_type, content));
            }
            _ => {
                return Err(AppError::validation(
                    "request",
                    "must contain exactly one kind field and one file field",
                ));
            }
        }
    }
    let kind = kind.ok_or_else(|| AppError::validation("kind", "is required"))?;
    let (filename, content_type, content) =
        upload.ok_or_else(|| AppError::validation("file", "is required"))?;
    let storage = state.object_storage().ok_or_else(|| {
        AppError::service_unavailable(
            "OBJECT_STORAGE_UNAVAILABLE",
            "Object storage is not configured",
        )
    })?;
    let attachment = state
        .problems()
        .upload_attachment(
            problem_id,
            kind,
            filename,
            content_type,
            content,
            context.user(),
            peer.ip(),
            storage,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(attachment)))
}

#[utoipa::path(
    get,
    path = "/api/problems/{problem_id}/attachments/{attachment_id}",
    operation_id = "downloadProblemAttachment",
    tag = "problems",
    params(("problem_id" = i64, Path), ("attachment_id" = i64, Path)),
    responses(
        (status = 200, description = "Attachment bytes", body = Vec<u8>, content_type = "application/octet-stream"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Attachment was not found or is not visible", body = crate::error::ApiErrorBody),
        (status = 503, description = "Object storage is unavailable", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn download_attachment(
    context: AuthContext,
    State(state): State<AppState>,
    Path((problem_id, attachment_id)): Path<(i64, i64)>,
) -> Result<Response, AppError> {
    context.require_password_ready()?;
    let storage = require_storage(&state)?;
    let download = state
        .problems()
        .download_attachment_reference(problem_id, attachment_id, context.user())
        .await?;
    let stream = storage
        .backend()
        .get_stream_limited(storage.problem_bucket(), &download.object_key, 20 * 1024 * 1024)
        .await
        .map_err(|error| AppError::internal("stream problem attachment object", error))?;
    let mut response = Response::new(Body::from_stream(stream));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, safe_content_type(download.content_type.as_deref()));
    headers.insert(header::CONTENT_DISPOSITION, content_disposition(&download.filename)?);
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

#[utoipa::path(
    delete,
    path = "/api/problems/{problem_id}/attachments/{attachment_id}",
    operation_id = "deleteProblemAttachment",
    tag = "problems",
    params(("problem_id" = i64, Path), ("attachment_id" = i64, Path)),
    responses(
        (status = 204, description = "Attachment deleted"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Attachment was not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 503, description = "Object storage is unavailable", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn delete_attachment(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((problem_id, attachment_id)): Path<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    let storage = require_storage(&state)?;
    state
        .problems()
        .delete_attachment(problem_id, attachment_id, context.user(), peer.ip(), storage)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/problems/{problem_id}/testdata",
    operation_id = "uploadProblemTestdata",
    tag = "problems",
    params(("problem_id" = i64, Path)),
    request_body(content = inline(TestdataUploadRequest), content_type = "multipart/form-data"),
    responses(
        (status = 201, description = "Test data uploaded as a new immutable version", body = ProblemTestdataResponse),
        (status = 400, description = "Invalid ZIP multipart upload", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem was not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Problem test data is locked", body = crate::error::ApiErrorBody),
        (status = 503, description = "Object storage is unavailable", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn upload_testdata(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(problem_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<ProblemTestdataResponse>), AppError> {
    let mut upload = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::validation("request", "must be valid multipart data"))?
    {
        if field.name() != Some("file") || upload.is_some() {
            return Err(AppError::validation("request", "must contain exactly one file field"));
        }
        let filename = validate_attachment_filename(
            field
                .file_name()
                .ok_or_else(|| AppError::validation("file", "must have a filename"))?
                .to_owned(),
        )?;
        if !filename.to_ascii_lowercase().ends_with(".zip") {
            return Err(AppError::validation("file", "filename must end with .zip"));
        }
        if let Some(content_type) = field.content_type()
            && !matches!(content_type, "application/zip" | "application/octet-stream")
        {
            return Err(AppError::validation("file", "media type must be application/zip"));
        }
        upload = Some(stage_testdata_field(&mut field).await?);
    }
    let (staging_file, bytes, sha256) = upload
        .ok_or_else(|| AppError::validation("request", "must contain exactly one file field"))?;
    let storage = require_storage(&state)?;
    let response = state
        .problems()
        .upload_testdata(
            problem_id,
            crate::features::problems::service::StagedTestdataUpload {
                path: staging_file.path().to_owned(),
                bytes,
                sha256,
            },
            context.user(),
            peer.ip(),
            storage,
        )
        .await;
    drop(staging_file);
    Ok((StatusCode::CREATED, Json(response?)))
}

#[utoipa::path(post, path = "/api/problems/{problem_id}/interactor", operation_id = "uploadProblemInteractor", tag = "problems", params(("problem_id" = i64, Path)), request_body(content = inline(InteractorUploadRequest), content_type = "multipart/form-data"), responses((status = 200, body = ProblemResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody), (status = 503, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn upload_interactor(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(problem_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<ProblemResponse>, AppError> {
    let mut upload = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::validation("request", "must be valid multipart data"))?
    {
        if field.name() != Some("file") || upload.is_some() {
            return Err(AppError::validation("request", "must contain exactly one file field"));
        }
        upload = Some(
            field.bytes().await.map_err(|_| AppError::validation("file", "could not be read"))?,
        );
    }
    let content = upload
        .ok_or_else(|| AppError::validation("request", "must contain exactly one file field"))?;
    let storage = require_storage(&state)?;
    Ok(Json(
        state
            .problems()
            .upload_interactor(problem_id, content, context.user(), peer.ip(), storage)
            .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/api/problems/{problem_id}/testdata",
    operation_id = "downloadProblemTestdata",
    tag = "problems",
    params(("problem_id" = i64, Path)),
    responses(
        (status = 200, description = "Active test data ZIP", body = Vec<u8>, content_type = "application/zip"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem or active test data was not found", body = crate::error::ApiErrorBody),
        (status = 503, description = "Object storage is unavailable", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn download_testdata(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(problem_id): Path<i64>,
) -> Result<Response, AppError> {
    let storage = require_storage(&state)?;
    let download = state.problems().download_testdata(problem_id, context.user(), storage).await?;
    let mut response = Response::new(download.content);
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    headers.insert(header::CONTENT_DISPOSITION, content_disposition(&download.filename)?);
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

#[utoipa::path(
    get,
    path = "/api/problems/{problem_id}/testdata/versions",
    operation_id = "listProblemTestdataVersions",
    tag = "problems",
    params(("problem_id" = i64, Path)),
    responses(
        (status = 200, description = "Immutable test data versions", body = [ProblemTestdataVersionResponse]),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem was not found or outside management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn list_testdata_versions(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(problem_id): Path<i64>,
) -> Result<Json<Vec<ProblemTestdataVersionResponse>>, AppError> {
    Ok(Json(state.problems().list_testdata_versions(problem_id, context.user()).await?))
}

#[utoipa::path(
    get,
    path = "/api/problems/{problem_id}/testdata/versions/{version}",
    operation_id = "downloadProblemTestdataVersion",
    tag = "problems",
    params(("problem_id" = i64, Path), ("version" = i32, Path)),
    responses(
        (status = 200, description = "Versioned test data ZIP", body = Vec<u8>, content_type = "application/zip"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem or test data version was not found", body = crate::error::ApiErrorBody),
        (status = 503, description = "Object storage is unavailable", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn download_testdata_version(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path((problem_id, version)): Path<(i64, i32)>,
) -> Result<Response, AppError> {
    let storage = require_storage(&state)?;
    let download = state
        .problems()
        .download_testdata_version(problem_id, version, context.user(), storage)
        .await?;
    let mut response = Response::new(download.content);
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    headers.insert(header::CONTENT_DISPOSITION, content_disposition(&download.filename)?);
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

#[utoipa::path(
    post,
    path = "/api/problems/{problem_id}/testdata/versions/{version}/activate",
    operation_id = "activateProblemTestdataVersion",
    tag = "problems",
    params(("problem_id" = i64, Path), ("version" = i32, Path)),
    request_body = ActivateTestdataVersionRequest,
    responses(
        (status = 200, description = "Test data version activated", body = ProblemTestdataVersionResponse),
        (status = 400, description = "Invalid expected current version", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Problem or test data version was not found", body = crate::error::ApiErrorBody),
        (status = 409, description = "Current test data version changed or problem is locked", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn activate_testdata_version(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((problem_id, version)): Path<(i64, i32)>,
    payload: Result<Json<ActivateTestdataVersionRequest>, JsonRejection>,
) -> Result<Json<ProblemTestdataVersionResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must contain expectedCurrentVersion"))?;
    Ok(Json(
        state
            .problems()
            .activate_testdata_version(
                problem_id,
                version,
                request.expected_current_version,
                context.user(),
                peer.ip(),
            )
            .await?,
    ))
}

fn require_storage(
    state: &AppState,
) -> Result<&crate::object_storage::ObjectStorageHandle, AppError> {
    state.object_storage().ok_or_else(|| {
        AppError::service_unavailable(
            "OBJECT_STORAGE_UNAVAILABLE",
            "Object storage is not configured",
        )
    })
}

fn safe_content_type(value: Option<&str>) -> HeaderValue {
    value
        .and_then(|value| HeaderValue::from_str(value).ok())
        .unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"))
}

fn content_disposition(filename: &str) -> Result<HeaderValue, AppError> {
    let encoded = filename.as_bytes().iter().fold(String::new(), |mut output, byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(*byte));
        } else {
            use std::fmt::Write;
            let _result = write!(output, "%{byte:02X}");
        }
        output
    });
    HeaderValue::from_str(&format!(
        "attachment; filename=\"attachment\"; filename*=UTF-8''{encoded}"
    ))
    .map_err(|error| AppError::internal("build attachment content disposition", error))
}

fn validate_attachment_content_type(content_type: Option<&str>) -> Result<(), AppError> {
    const ALLOWED: [&str; 7] = [
        "application/octet-stream",
        "application/pdf",
        "application/zip",
        "image/jpeg",
        "image/png",
        "text/markdown",
        "text/plain",
    ];
    if content_type.is_none_or(|value| ALLOWED.contains(&value)) {
        Ok(())
    } else {
        Err(AppError::validation("file", "has an unsupported content type"))
    }
}

/// Backing file for a streamed test-data upload. Removal happens on drop so
/// every early-return path in the handler leaves no staging debris behind.
struct StagedUploadFile(std::path::PathBuf);

impl StagedUploadFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for StagedUploadFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Streams one multipart field to a private temporary file while hashing and
/// size-capping the bytes as they arrive. A full 256 MiB archive therefore
/// never materialises in request memory, and an oversized upload is rejected
/// as soon as the streamed total crosses the cap.
async fn stage_testdata_field(
    field: &mut axum::extract::multipart::Field<'_>,
) -> Result<(StagedUploadFile, u64, String), AppError> {
    use sha2::Digest as _;
    use tokio::io::AsyncWriteExt as _;

    const MAX_TESTDATA_BYTES: u64 = 256 * 1024 * 1024;
    let staging_dir = std::env::temp_dir().join("project-balloon-uploads");
    tokio::fs::create_dir_all(&staging_dir)
        .await
        .map_err(|error| AppError::internal("prepare test-data staging directory", error))?;
    let path = staging_dir.join(format!("testdata-{}.zip", uuid::Uuid::new_v4()));
    let mut open_options = std::fs::OpenOptions::new();
    open_options.write(true).create_new(true);
    // Only POSIX platforms carry the mode bits that keep the staged upload
    // readable by its owner alone; Windows denies other users by default.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        open_options.mode(0o600);
    }
    let file = open_options
        .open(&path)
        .map_err(|error| AppError::internal("create test-data staging file", error))?;
    let mut writer = tokio::fs::File::from(file);
    let mut hasher = sha2::Sha256::new();
    let mut total: u64 = 0;
    while let Some(chunk) =
        field.chunk().await.map_err(|_| AppError::validation("file", "could not be read"))?
    {
        total = total.saturating_add(chunk.len() as u64);
        if total > MAX_TESTDATA_BYTES {
            return Err(AppError::validation("file", "must contain between 1 byte and 256 MiB"));
        }
        hasher.update(&chunk);
        writer
            .write_all(&chunk)
            .await
            .map_err(|error| AppError::internal("stage test-data upload", error))?;
    }
    if total == 0 {
        return Err(AppError::validation("file", "must contain between 1 byte and 256 MiB"));
    }
    writer.flush().await.map_err(|error| AppError::internal("stage test-data upload", error))?;
    Ok((StagedUploadFile(path), total, hex::encode(hasher.finalize())))
}
