use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::cash_categories::{Payee, UpdatePayeeRequest};
use crate::modules::payees::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(
    patch,
    path = "/{key}",
    params(("key" = String, Path, description = "Payee key (`pye_…`)")),
    request_body = UpdatePayeeRequest,
    responses(
        (status = 200, description = "Payee updated", body = ApiResponse<Payee>),
        (status = 400, description = "Suppliers and staff are changed on their own pages", body = ErrorResponse),
        (status = 404, description = "Payee not found", body = ErrorResponse),
        (status = 409, description = "Name already in the list", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Payees"
)]
pub(super) async fn update_payee(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<UpdatePayeeRequest>,
) -> AppResult<Json<ApiResponse<Payee>>> {
    current_user.require_permission(perm::PAYEES_MANAGE)?;
    let payee = service::update_payee(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(
        payee,
        "Payee updated successfully",
    )))
}
