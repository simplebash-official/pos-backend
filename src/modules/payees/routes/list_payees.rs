use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::cash_categories::{Payee, PayeeListQuery};
use crate::modules::payees::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/",
    params(PayeeListQuery),
    responses((status = 200, description = "Suppliers, staff and saved payees, frequent first", body = ApiResponse<Vec<Payee>>)),
    security(("bearerAuth" = [])),
    tag = "Payees"
)]
pub(super) async fn list_payees(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<PayeeListQuery>,
) -> AppResult<Json<ApiResponse<Vec<Payee>>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    if query.include_hidden == Some(true) {
        current_user.require_permission(perm::PAYEES_MANAGE)?;
    }
    let payees = service::list_payees(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        payees,
        "Payees retrieved successfully",
    )))
}
