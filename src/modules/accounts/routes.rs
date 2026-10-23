// HTTP layer for the money accounts.

use axum::{
    Json,
    extract::{Path, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::permissions as perm,
        error::{AppError, AppResult},
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::accounts::{AccountKind, AccountSummary, AccountsResponse, SetAccountOpeningRequest},
    modules::accounts::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_accounts))
        // Writes
        .routes(routes!(set_account_opening))
}

// ============================================================================
// Accounts
// ============================================================================

#[utoipa::path(
    get,
    path = "/",
    responses((status = 200, description = "The drawer, bank, card and online accounts with their balances", body = ApiResponse<AccountsResponse>)),
    security(("bearerAuth" = [])),
    tag = "Accounts"
)]
async fn list_accounts(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<AccountsResponse>>> {
    current_user.require_permission(perm::ACCOUNTS_VIEW)?;
    let accounts = service::list_accounts(&state.db).await?;
    Ok(Json(ApiResponse::success(accounts, "Accounts retrieved successfully")))
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
async fn set_account_opening(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(kind): Path<String>,
    Json(body): Json<SetAccountOpeningRequest>,
) -> AppResult<Json<ApiResponse<AccountSummary>>> {
    current_user.require_permission(perm::ACCOUNTS_MANAGE)?;
    let kind = AccountKind::parse(&kind).ok_or_else(|| AppError::validation("Unknown account."))?;
    let name = current_user.resolve_name(&state.db).await;
    let summary = service::set_opening(&state.db, kind, name, body).await?;
    Ok(Json(ApiResponse::success(summary, "Opening balance saved successfully")))
}
