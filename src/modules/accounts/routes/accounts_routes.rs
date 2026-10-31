use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::accounts::{
    AccountKind, AccountSummary, AccountsResponse, SetAccountOpeningRequest,
};
use crate::modules::accounts::service;
use axum::Json;
use axum::extract::{Path, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

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

/// Routes of the accounts module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_accounts))
        // Writes
        .routes(routes!(set_account_opening))
}

#[utoipa::path(
    put,
    path = "/{kind}/opening",
    params(("kind" = String, Path, description = "bank | card | online")),
    request_body = SetAccountOpeningRequest,
    responses(
        (status = 200, description = "The account with its new opening balance", body = ApiResponse<AccountSummary>),
        (status = 400, description = "Validation failed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Accounts"
)]
pub(super) async fn set_account_opening(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(kind): Path<String>,
    Json(body): Json<SetAccountOpeningRequest>,
) -> AppResult<Json<ApiResponse<AccountSummary>>> {
    current_user.require_permission(perm::ACCOUNTS_MANAGE)?;
    let kind = AccountKind::parse(&kind).ok_or_else(|| AppError::validation("Unknown account."))?;
    let name = current_user.resolve_name(&state.db).await?;
    let summary = service::set_opening(&state.db, kind, name, body).await?;
    Ok(Json(ApiResponse::success(
        summary,
        "Opening balance saved successfully",
    )))
}
