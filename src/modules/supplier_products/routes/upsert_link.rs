use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::supplier_products::{SupplierProductLink, UpsertSupplierProductLinkRequest};
use crate::modules::supplier_products::service::link;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(post, path = "/", tag = modules::SUPPLIER_PRODUCTS, request_body = UpsertSupplierProductLinkRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Link created or updated", body = ApiResponse<SupplierProductLink>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier or product not found", body = ErrorResponse),
    )
)]
pub(super) async fn upsert_link(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UpsertSupplierProductLinkRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<SupplierProductLink>>)> {
    let link = link::upsert_link(&state.db, body).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(link, "Product linked to supplier")),
    ))
}
