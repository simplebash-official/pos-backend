use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::accounts::AccountsResponse;
use crate::modules::accounts::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(
    get,
    path = "/",
    responses((status = 200, description = "The drawer, bank, card and online accounts with their balances", body = ApiResponse<AccountsResponse>)),
    security(("bearerAuth" = [])),
    tag = "Accounts"
)]
pub(super) async fn list_accounts(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<AccountsResponse>>> {
    current_user.require_permission(perm::ACCOUNTS_VIEW)?;
    let accounts = service::list_accounts(&state.db).await?;
    Ok(Json(ApiResponse::success(
        accounts,
        "Accounts retrieved successfully",
    )))
}
