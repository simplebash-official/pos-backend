use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;

use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::supplier_credits::SupplierCredit;
use crate::modules::purchases::service::credit;
use crate::modules::purchases::service::receipt::RecordedBy;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

#[utoipa::path(post, path = "/{key}/void", tag = modules::SUPPLIER_CREDITS,
    params(("key" = String, Path, description = "Credit key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Credit voided by a reversing entry", body = ApiResponse<SupplierCredit>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Credit not found", body = ErrorResponse),
        (status = 409, description = "Already voided", body = ErrorResponse),
    )
)]
pub(super) async fn void_credit(
    AdminUser(current_user): AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<(StatusCode, Json<ApiResponse<SupplierCredit>>)> {
    let by = RecordedBy {
        name: current_user.resolve_name(&state.db).await,
        user_id: current_user.user_id,
    };
    let voided = credit::void_credit(&state.db, by, &key).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(voided, "Credit voided successfully")),
    ))
}
