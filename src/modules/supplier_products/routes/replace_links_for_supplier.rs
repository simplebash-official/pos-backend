use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::supplier_products::{BulkReplaceLinksRequest, SupplierProductLink};
use crate::modules::supplier_products::service::link;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(put, path = "/bulk/{supplierKey}", tag = modules::SUPPLIER_PRODUCTS,
    params(("supplierKey" = String, Path, description = "Supplier key")),
    request_body = BulkReplaceLinksRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Supplier's product links replaced", body = ApiResponse<Vec<SupplierProductLink>>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier or one of the products not found", body = ErrorResponse),
    )
)]
pub(super) async fn replace_links_for_supplier(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(supplier_key): Path<String>,
    Json(body): Json<BulkReplaceLinksRequest>,
) -> AppResult<Json<ApiResponse<Vec<SupplierProductLink>>>> {
    let links = link::replace_links_for_supplier(&state.db, supplier_key, body).await?;

    Ok(Json(ApiResponse::success(
        links,
        "Supplier product links updated",
    )))
}
