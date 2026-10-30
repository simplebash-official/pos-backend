use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::finance_reports::PayablesResponse;
use crate::modules::finance_reports::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(
    get,
    path = "/payables",
    responses((status = 200, description = "Bills and supplier balances still to pay", body = ApiResponse<PayablesResponse>)),
    security(("bearerAuth" = [])),
    tag = "Finance Reports"
)]
pub(super) async fn payables(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<PayablesResponse>>> {
    current_user.require_permission(perm::REPORTS_VIEW)?;
    let report = service::payables(&state.db).await?;
    Ok(Json(ApiResponse::success(
        report,
        "Payables retrieved successfully",
    )))
}
