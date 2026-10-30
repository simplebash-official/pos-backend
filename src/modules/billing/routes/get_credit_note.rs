use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::billing::CreditNote;

use crate::modules::billing::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(get, path = "/credit-notes/{id}", tag = modules::BILLING,
    params(("id" = String, Path, description = "Credit note ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a credit note record", body = ApiResponse<CreditNote>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Credit note not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_credit_note(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<CreditNote>>> {
    let credit_note = service::credit_notes::get_credit_note(&state.db, &id).await?;
    Ok(Json(ApiResponse::success(
        credit_note,
        "Credit note retrieved successfully",
    )))
}
