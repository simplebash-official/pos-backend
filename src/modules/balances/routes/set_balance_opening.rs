use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::balances::{BalanceSubject, SetBalanceOpeningRequest};
use crate::modules::balances::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(
    put,
    path = "/{kind}/{key}/opening",
    params(
        ("kind" = String, Path, description = "supplier | staff"),
        ("key" = String, Path, description = "Supplier or employee key")
    ),
    request_body = SetBalanceOpeningRequest,
    responses(
        (status = 200, description = "Opening balance saved", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Validation failed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Balances"
)]
pub(super) async fn set_balance_opening(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path((kind, key)): Path<(String, String)>,
    Json(body): Json<SetBalanceOpeningRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    current_user.require_permission(perm::BALANCES_MANAGE)?;
    let subject =
        BalanceSubject::parse(&kind).ok_or_else(|| AppError::validation("Unknown kind."))?;
    let name = current_user.resolve_name(&state.db).await;
    service::set_opening(&state.db, subject, &key, name, body).await?;
    Ok(Json(ApiResponse::success(
        serde_json::json!({}),
        "Opening balance saved successfully",
    )))
}
