// HTTP layer for the payee directory.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::permissions as perm,
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::cash_categories::{CreatePayeeRequest, Payee, PayeeListQuery},
    modules::payees::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_payees))
        .routes(routes!(create_payee))
}

// ============================================================================
// Payees
// ============================================================================

#[utoipa::path(
    get,
    path = "/",
    params(PayeeListQuery),
    responses((status = 200, description = "Suppliers, staff and saved payees, frequent first", body = ApiResponse<Vec<Payee>>)),
    security(("bearerAuth" = [])),
    tag = "Payees"
)]
async fn list_payees(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<PayeeListQuery>,
) -> AppResult<Json<ApiResponse<Vec<Payee>>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    let payees = service::list_payees(&state.db, query).await?;
    Ok(Json(ApiResponse::success(payees, "Payees retrieved successfully")))
}

#[utoipa::path(
    post,
    path = "/",
    request_body = CreatePayeeRequest,
    responses(
        (status = 201, description = "Payee created", body = ApiResponse<Payee>),
        (status = 409, description = "Name already in the list", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Payees"
)]
async fn create_payee(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<CreatePayeeRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Payee>>)> {
    current_user.require_permission(perm::PAYEES_MANAGE)?;
    let payee = service::create_payee(&state.db, body).await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::success(payee, "Payee created successfully"))))
}
