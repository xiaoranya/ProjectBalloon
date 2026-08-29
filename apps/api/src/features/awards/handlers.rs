use super::*;

use crate::features::awards::service::require_operator;

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
    let mut out = "categoryCode,categoryName,rank,teamId,teamName,school,participationType,groupName,manual\n".to_string();
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

pub(super) fn percent_encode_filename(value: &str) -> String {
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

pub(super) fn csv_field(v: &str) -> String {
    let safe = if matches!(v.as_bytes().first(), Some(b'=' | b'+' | b'-' | b'@')) {
        format!("'{v}")
    } else {
        v.to_owned()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}
