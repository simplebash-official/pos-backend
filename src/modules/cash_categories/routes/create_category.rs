use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::cash_categories::{CashCategory, CreateCashCategoryRequest};
use crate::modules::cash_categories::service;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(
    post,
    path = "/",
    request_body = CreateCashCategoryRequest,
    responses(
        (status = 201, description = "Category created", body = ApiResponse<CashCategory>),
        (status = 403, description = "Not allowed for this kind of payment", body = ErrorResponse),
        (status = 409, description = "Name already used", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Categories"
)]
pub(super) async fn create_category(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<CreateCashCategoryRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CashCategory>>)> {
    current_user.require_permission(perm::CASH_CATEGORIES_CREATE)?;
    let name = current_user.resolve_name(&state.db).await;
    let can_manage = current_user.has_permission(perm::CASH_CATEGORIES_MANAGE);
    let category = service::create_category(&state.db, name, can_manage, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            category,
            "Category created successfully",
        )),
    ))
}
