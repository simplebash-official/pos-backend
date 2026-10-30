use super::product_for::product_for;

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::SerialLookup;

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/serials/by-number/{serialNumber}", tag = modules::INVENTORY,
    params(("serialNumber" = String, Path, description = "Exact serial number, as printed on a serial label")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The in-stock unit with this serial number, its product and selling price", body = ApiResponse<SerialLookup>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "SERIAL_NOT_FOUND: no unit has this serial number", body = ErrorResponse),
        (status = 409, description = "SERIAL_ALREADY_SOLD: the unit is not in stock", body = ErrorResponse),
    )
)]
pub(super) async fn get_serial_by_number(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(serial_number): Path<String>,
) -> AppResult<Json<ApiResponse<SerialLookup>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let mut lookup =
        service::product_serial::lookup_in_stock_serial(&state.db, &serial_number).await?;
    lookup.product = product_for(&user, lookup.product);

    Ok(Json(ApiResponse::success(
        lookup,
        "Serial number retrieved successfully",
    )))
}
