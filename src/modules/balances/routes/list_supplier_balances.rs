use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::balances::SupplierBalancesResponse;
use crate::modules::balances::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(
    get,
    path = "/suppliers",
    responses((status = 200, description = "Every supplier with what the shop owes them", body = ApiResponse<SupplierBalancesResponse>)),
    security(("bearerAuth" = [])),
    tag = "Balances"
)]
pub(super) async fn list_supplier_balances(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<SupplierBalancesResponse>>> {
    current_user.require_permission(perm::BALANCES_VIEW)?;
    let response = service::list_supplier_balances(&state.db).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Supplier balances retrieved successfully",
    )))
}
