use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::quotations::Quotation;

use crate::modules::quotations::service;
use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(get, path = "/{id}", tag = modules::QUOTATIONS,
    params(("id" = String, Path, description = "Quotation ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a quotation", body = ApiResponse<Quotation>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Quotation not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_quotation(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Quotation>>> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let quotation = service::get_quotation(&state.db, &id).await?;
    Ok(Json(ApiResponse::success(
        quotation,
        "Quotation retrieved successfully",
    )))
}
