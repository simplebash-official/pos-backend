// HTTP layer only: extractors, permission checks and OpenAPI docs. Every
// handler makes one `service::*` call; the rules live in `service/`.

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
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
        LabelTemplate, LabelTemplateInput, LabelTemplatesResponse, PreviewLabelsRequest,
        RenderLabelsRequest,
    },
    modules::labels::service,
};

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
async fn render_labels(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<RenderLabelsRequest>,
) -> AppResult<Response> {
    user.require_permission(perm::INVENTORY_READ)?;
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
