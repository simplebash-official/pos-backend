// HTTP route handlers and OpenAPI declarations for settings and shop profiles.

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
        constants::modules,
        error::AppResult,
        middleware::auth::{AdminUser, CurrentUser},
        response::{ApiResponse, ErrorResponse},
    },
    domain::settings::{
        DocumentTemplatesResponse, ShopProfileResponse, UpdateDocumentTemplatesRequest,
        UpdateShopProfileRequest,
    },
    modules::settings::service,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_shop_profile, update_shop_profile))
        .routes(routes!(get_document_templates, update_document_templates))
        .routes(routes!(get_document_template_preview))
}

/// Retrieve the active shop profile.
#[utoipa::path(
    get,
    path = "/shop-profile",
    tag = modules::SETTINGS,
    responses(
        (status = 200, description = "Shop profile retrieved successfully", body = ApiResponse<ShopProfileResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_shop_profile(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<ShopProfileResponse>>> {
    let profile = service::get_shop_profile(&state.db, user.tenant_id.as_deref()).await?;
    Ok(Json(ApiResponse::success(
        profile,
        "Shop profile retrieved successfully",
    )))
}

/// Update the active shop profile.
#[utoipa::path(
    put,
    path = "/shop-profile",
    tag = modules::SETTINGS,
    request_body = UpdateShopProfileRequest,
    responses(
        (status = 200, description = "Shop profile updated successfully", body = ApiResponse<ShopProfileResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin role required", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn update_shop_profile(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateShopProfileRequest>,
) -> AppResult<Json<ApiResponse<ShopProfileResponse>>> {
    let profile =
        service::update_shop_profile(&state.db, admin.0.tenant_id.as_deref(), body).await?;
    Ok(Json(ApiResponse::success(
        profile,
        "Shop profile updated successfully",
    )))
}

/// The shop's document designs: for each kind (A4 invoice, receipt, credit
/// note) the designs available and which one is in use.
#[utoipa::path(
    get,
    path = "/document-templates",
    tag = modules::SETTINGS,
    responses(
        (status = 200, description = "Document designs retrieved successfully", body = ApiResponse<DocumentTemplatesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_document_templates(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<DocumentTemplatesResponse>>> {
    let response = service::get_document_templates(&state.db, &state.document_server).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Document designs retrieved successfully",
    )))
}

/// Choose the shop's designs and set their option values. Only the kinds and
/// designs in the request change; `null` returns a kind to the standard design.
#[utoipa::path(
    put,
    path = "/document-templates",
    tag = modules::SETTINGS,
    request_body = UpdateDocumentTemplatesRequest,
    responses(
        (status = 200, description = "Document designs updated successfully", body = ApiResponse<DocumentTemplatesResponse>),
        (status = 400, description = "Unknown document kind", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin role required", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: that design does not exist or is not for this kind of document; TEMPLATE_OPTIONS_INVALID: an unknown option or a value its control does not accept", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn update_document_templates(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateDocumentTemplatesRequest>,
) -> AppResult<Json<ApiResponse<DocumentTemplatesResponse>>> {
    let response =
        service::update_document_templates(&state.db, &state.document_server, body).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Document designs updated successfully",
    )))
}

/// A PDF of one design rendered with its own sample data.
#[utoipa::path(
    get,
    path = "/document-templates/{family}/preview",
    tag = modules::SETTINGS,
    params(("family" = String, Path, description = "The design family, as listed by GET /settings/document-templates")),
    responses(
        (status = 200, description = "Sample PDF", content_type = "application/pdf", body = Vec<u8>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "TEMPLATE_UNAVAILABLE: no such design, or it has no sample", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_document_template_preview(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(family): Path<String>,
) -> AppResult<Response> {
    let pdf = service::render_document_template_preview(&state.document_server, &family).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(pdf),
    )
        .into_response())
}
