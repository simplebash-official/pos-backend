use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::billing::{CreditNoteListQuery, CreditNoteListResponse};

use crate::modules::billing::service;

use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(get, path = "/credit-notes", tag = modules::BILLING, params(CreditNoteListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List credit note records with search, date/invoice/customer filtering, and pagination", body = ApiResponse<CreditNoteListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn list_credit_notes(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<CreditNoteListQuery>,
) -> AppResult<Json<ApiResponse<CreditNoteListResponse>>> {
    let response = service::credit_notes::list_credit_notes(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Credit notes retrieved successfully",
    )))
}
