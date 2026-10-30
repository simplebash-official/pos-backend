use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::cash_categories::{CreatePayeeRequest, Payee};
use crate::modules::payees::service;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

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
pub(super) async fn create_payee(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<CreatePayeeRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Payee>>)> {
    current_user.require_permission(perm::PAYEES_MANAGE)?;
    let payee = service::create_payee(&state.db, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(payee, "Payee created successfully")),
    ))
}
