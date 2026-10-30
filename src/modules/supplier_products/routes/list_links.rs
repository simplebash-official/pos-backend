use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::supplier_products::{SupplierProductLinkQuery, SupplierProductListResponse};
use crate::modules::supplier_products::service::link;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/", tag = modules::SUPPLIER_PRODUCTS, params(SupplierProductLinkQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Links for a supplier and/or product, or full collection paginated", body = ApiResponse<SupplierProductListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn list_links(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<SupplierProductLinkQuery>,
) -> AppResult<Json<ApiResponse<SupplierProductListResponse>>> {
    let response = link::list_links(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Supplier-product links retrieved successfully",
    )))
}
