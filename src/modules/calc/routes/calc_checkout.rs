use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::calc::checkout::{CalcCheckoutReply, CalcCheckoutRequest};

use crate::modules::calc::checkout;
use axum::Json;

#[utoipa::path(post, path = "/checkout", tag = modules::CALC, request_body = CalcCheckoutRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Every line, the totals, the payment state and the quick-tender buttons for the cart as it stands", body = ApiResponse<CalcCheckoutReply>),
        (status = 400, description = "An amount or count is out of range, or there are too many lines", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Same access as making a sale (`billing:write`).
pub(super) async fn calc_checkout(
    user: CurrentUser,
    Json(body): Json<CalcCheckoutRequest>,
) -> AppResult<Json<ApiResponse<CalcCheckoutReply>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let reply = checkout::compute_checkout(&body)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
