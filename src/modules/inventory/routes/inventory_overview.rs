use super::product_for::product_for;

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{InventoryOverviewQuery, InventoryOverviewResponse};

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/overview", tag = modules::INVENTORY, params(InventoryOverviewQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get inventory overview with metrics, category hierarchy, subcategory data tables, search, and filtration", body = ApiResponse<InventoryOverviewResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn inventory_overview(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<InventoryOverviewQuery>,
) -> AppResult<Json<ApiResponse<InventoryOverviewResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let mut response = service::overview::get_inventory_overview(&state.db, query).await?;
    for category in &mut response.categories {
        for subcategory in &mut category.subcategories {
            subcategory.products = std::mem::take(&mut subcategory.products)
                .into_iter()
                .map(|product| product_for(&user, product))
                .collect();
        }
    }

    Ok(Json(ApiResponse::success(
        response,
        "Inventory overview retrieved successfully",
    )))
}
