use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::calc::quotation::{CalcQuotationReply, CalcQuotationRequest};

use crate::modules::calc::quotation;
use axum::Json;

#[utoipa::path(post, path = "/quotation", tag = modules::CALC, request_body = CalcQuotationRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Each line's discount and total, the subtotal, order discount and total, and what still stops the quotation being created", body = ApiResponse<CalcQuotationReply>),
        (status = 400, description = "An amount or count is out of range, or there are too many lines", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Same access as creating a quotation (`quotations:manage`).
pub(super) async fn calc_quotation(
    user: CurrentUser,
    Json(body): Json<CalcQuotationRequest>,
) -> AppResult<Json<ApiResponse<CalcQuotationReply>>> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let reply = quotation::compute(&body, chrono::Utc::now().date_naive())?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
