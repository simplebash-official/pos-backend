// HTTP layer only: extractors, permission checks and OpenAPI docs. Every
// handler makes one `service::*` call; the rules live in `service/`.

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::{modules, permissions as perm},
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::labels::{
        CreateLabelJobRequest, LabelCoverageQuery, LabelCoverageResponse, LabelJob,
        LabelJobListQuery, LabelJobListResponse, LabelTemplate, LabelTemplateInput,
        LabelTemplatesResponse, PreviewLabelsRequest, RenderLabelsRequest, UpdateLabelJobRequest,
    },
    domain::users::Role,
    modules::{labels::service, users},
};

/// Response header carrying the key of the job `POST /labels/jobs` recorded
/// (exposed to browsers by the CORS layer in `app.rs`).
pub const LABEL_JOB_KEY_HEADER: &str = "x-label-job-key";

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Designs
        .routes(routes!(list_templates, create_template))
        .routes(routes!(update_template, delete_template))
        // Printing
        .routes(routes!(render_labels))
        .routes(routes!(preview_label))
        // Print jobs
        .routes(routes!(list_jobs, create_job))
        .routes(routes!(get_job, report_job))
        .routes(routes!(coverage))
}

// ============================================================================
// Helpers
// ============================================================================

fn pdf_response(bytes: Vec<u8>) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(bytes),
    )
        .into_response()
}

/// The printing person's name for the history: the token's own name (identity
/// tokens carry one), else the account's, else "Unknown" (same as billing's
/// recorded-by snapshot).
async fn printed_by_name(state: &AppState, user: &CurrentUser) -> String {
    if let Some(name) = user.name.as_deref().filter(|n| !n.trim().is_empty()) {
        return name.to_string();
    }
    match mongodb::bson::oid::ObjectId::parse_str(&user.user_id) {
        Ok(id) => users::service::get_user(&state.db, id)
            .await
            .map(|u| u.name)
            .unwrap_or_else(|_| "Unknown".to_string()),
        Err(_) => "Unknown".to_string(),
    }
}

// ============================================================================
// Designs
// ============================================================================

#[utoipa::path(get, path = "/templates", tag = modules::LABELS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The ready-made designs, then the shop's own", body = ApiResponse<LabelTemplatesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn list_templates(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<LabelTemplatesResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let templates = service::list_templates(&state.db).await?;
    Ok(Json(ApiResponse::success(
        templates,
        "Label designs retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/templates", tag = modules::LABELS, request_body = LabelTemplateInput,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Design saved", body = ApiResponse<LabelTemplate>),
        (status = 400, description = "The design is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 409, description = "LABEL_TEMPLATE_NAME_TAKEN: another design has this name", body = ErrorResponse),
    )
)]
async fn create_template(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<LabelTemplateInput>,
) -> AppResult<(StatusCode, Json<ApiResponse<LabelTemplate>>)> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let template = service::create_template(&state.db, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            template,
            "Label design saved successfully",
        )),
    ))
}

#[utoipa::path(put, path = "/templates/{key}", tag = modules::LABELS, request_body = LabelTemplateInput,
    params(("key" = String, Path, description = "Design key (ltpl_...)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Design saved", body = ApiResponse<LabelTemplate>),
        (status = 400, description = "The design is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "LABEL_TEMPLATE_NOT_FOUND", body = ErrorResponse),
        (status = 409, description = "LABEL_TEMPLATE_BUILT_IN or LABEL_TEMPLATE_NAME_TAKEN", body = ErrorResponse),
    )
)]
async fn update_template(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<LabelTemplateInput>,
) -> AppResult<Json<ApiResponse<LabelTemplate>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let template = service::update_template(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(
        template,
        "Label design saved successfully",
    )))
}

#[utoipa::path(delete, path = "/templates/{key}", tag = modules::LABELS,
    params(("key" = String, Path, description = "Design key (ltpl_...)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Design deleted"),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "LABEL_TEMPLATE_NOT_FOUND", body = ErrorResponse),
        (status = 409, description = "LABEL_TEMPLATE_BUILT_IN: ready-made designs cannot be deleted", body = ErrorResponse),
    )
)]
async fn delete_template(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<()>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    service::delete_template(&state.db, &key).await?;
    Ok(Json(ApiResponse::message(
        "Label design deleted successfully",
    )))
}

// ============================================================================
// Printing
// ============================================================================

#[utoipa::path(post, path = "/render", tag = modules::LABELS, request_body = RenderLabelsRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "A PDF with one page per sticker", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Nothing to print, or too many labels", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Design, product or serial number not found", body = ErrorResponse),
        (status = 409, description = "SERIAL_ALREADY_SOLD: the unit is not in stock", body = ErrorResponse),
        (status = 422, description = "LABEL_PRODUCT_WITHOUT_BARCODE, LABEL_PRICE_REQUIRED, PRICE_TIER_UNAVAILABLE, LABEL_BARCODE_TOO_NARROW or LABEL_QR_TOO_SMALL", body = ErrorResponse),
    )
)]
/// The print window's preview: renders without recording anything. Real
/// prints go through `POST /labels/jobs`. Admin and Manager only.
async fn render_labels(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<RenderLabelsRequest>,
) -> AppResult<Response> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let pdf = service::render_labels(
        &state.db,
        &state.document_server,
        user.tenant_id.as_deref(),
        body,
    )
    .await?;
    Ok(pdf_response(pdf))
}

#[utoipa::path(post, path = "/preview", tag = modules::LABELS, request_body = PreviewLabelsRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "A one-page PDF of the unsaved design drawn with the sample", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "The design is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 422, description = "LABEL_BARCODE_TOO_NARROW or LABEL_QR_TOO_SMALL", body = ErrorResponse),
    )
)]
async fn preview_label(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<PreviewLabelsRequest>,
) -> AppResult<Response> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let pdf = service::preview_label(
        &state.db,
        &state.document_server,
        user.tenant_id.as_deref(),
        body,
    )
    .await?;
    Ok(pdf_response(pdf))
}

// ============================================================================
// Print jobs
// ============================================================================

#[utoipa::path(get, path = "/jobs", tag = modules::LABELS, params(LabelJobListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Recorded prints and downloads, newest first", body = ApiResponse<LabelJobListResponse>),
        (status = 400, description = "A date is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn list_jobs(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<LabelJobListQuery>,
) -> AppResult<Json<ApiResponse<LabelJobListResponse>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let jobs = service::jobs::list_jobs(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        jobs,
        "Label prints retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/jobs", tag = modules::LABELS, request_body = CreateLabelJobRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The PDF to print or save (one page per sticker); the recorded job's key is in the X-Label-Job-Key header", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Nothing to print, too many labels, or the print method is incomplete", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Design, product or serial number not found", body = ErrorResponse),
        (status = 409, description = "SERIAL_ALREADY_SOLD: the unit is not in stock", body = ErrorResponse),
        (status = 422, description = "LABEL_PRODUCT_WITHOUT_BARCODE, LABEL_PRICE_REQUIRED, PRICE_TIER_UNAVAILABLE, LABEL_BARCODE_TOO_NARROW or LABEL_QR_TOO_SMALL", body = ErrorResponse),
    )
)]
async fn create_job(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateLabelJobRequest>,
) -> AppResult<Response> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let name = printed_by_name(&state, &user).await;
    let (job, pdf) = service::jobs::create_job(
        &state.db,
        &state.document_server,
        user.tenant_id.as_deref(),
        &user.user_id,
        &name,
        body,
    )
    .await?;
    let mut response = pdf_response(pdf);
    let key = HeaderValue::from_str(&job.key)
        .map_err(|e| crate::core::error::AppError::internal(format!("Bad job key: {e}")))?;
    response.headers_mut().insert(LABEL_JOB_KEY_HEADER, key);
    Ok(response)
}

#[utoipa::path(get, path = "/jobs/{key}", tag = modules::LABELS,
    params(("key" = String, Path, description = "Print job key (lpj_...)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "One recorded print", body = ApiResponse<LabelJob>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "LABEL_JOB_NOT_FOUND", body = ErrorResponse),
    )
)]
async fn get_job(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<LabelJob>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let job = service::jobs::get_job(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        job,
        "Label print retrieved successfully",
    )))
}

#[utoipa::path(patch, path = "/jobs/{key}", tag = modules::LABELS, request_body = UpdateLabelJobRequest,
    params(("key" = String, Path, description = "Print job key (lpj_...)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The printer step's result was recorded", body = ApiResponse<LabelJob>),
        (status = 400, description = "Outcome must be sent or failed; a reason only with failed", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "LABEL_JOB_NOT_FOUND", body = ErrorResponse),
        (status = 409, description = "LABEL_JOB_ALREADY_REPORTED: a download, or reported before", body = ErrorResponse),
    )
)]
async fn report_job(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<UpdateLabelJobRequest>,
) -> AppResult<Json<ApiResponse<LabelJob>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let job = service::jobs::report_job(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(
        job,
        "Label print updated successfully",
    )))
}

#[utoipa::path(get, path = "/coverage", tag = modules::LABELS, params(LabelCoverageQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Products with their label state: never labelled, labels needed for new stock, or labelled", body = ApiResponse<LabelCoverageResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn coverage(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<LabelCoverageQuery>,
) -> AppResult<Json<ApiResponse<LabelCoverageResponse>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let mut coverage = service::jobs::coverage(&state.db, query).await?;
    // What the shop paid stays Admin-only, as on the inventory routes.
    if user.role != Some(Role::Admin) {
        for row in &mut coverage.items {
            row.product = row.product.clone().without_cost();
        }
    }
    Ok(Json(ApiResponse::success(
        coverage,
        "Label coverage retrieved successfully",
    )))
}
