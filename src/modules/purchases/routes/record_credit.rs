use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;

use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::supplier_credits::{CreateSupplierCreditRequest, SupplierCredit};
use crate::modules::purchases::service::credit;
use crate::modules::purchases::service::receipt::RecordedBy;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(post, path = "/", tag = modules::SUPPLIER_CREDITS, request_body = CreateSupplierCreditRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Credit recorded", body = ApiResponse<SupplierCredit>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier or receipt not found", body = ErrorResponse),
        (status = 409, description = "More than the receipt still owes", body = ErrorResponse),
    )
)]
pub(super) async fn record_credit(
    AdminUser(current_user): AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateSupplierCreditRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<SupplierCredit>>)> {
    let by = RecordedBy {
        name: current_user.resolve_name(&state.db).await,
        user_id: current_user.user_id,
    };
    let recorded = credit::create_credit(&state.db, by, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            recorded,
            "Credit recorded successfully",
        )),
    ))
}
