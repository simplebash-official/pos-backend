use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::calc::earnings::{CalcEarningsReply, CalcEarningsRequest};

use crate::modules::calc::earnings;
use axum::Json;

#[utoipa::path(post, path = "/earnings", tag = modules::CALC, request_body = CalcEarningsRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The job's profit, the employee's commission, whether a fixed commission was cut down, and what the shop keeps", body = ApiResponse<CalcEarningsReply>),
        (status = 400, description = "An amount or the commission is out of range", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Needs `repairs:write`, `print_jobs:write` or `employees:read`: the job forms and the employee screen.
pub(super) async fn calc_earnings(
    user: CurrentUser,
    Json(body): Json<CalcEarningsRequest>,
) -> AppResult<Json<ApiResponse<CalcEarningsReply>>> {
    user.require_any_permission(&[
        perm::REPAIRS_WRITE,
        perm::PRINT_JOBS_WRITE,
        perm::EMPLOYEES_READ,
    ])?;
    let reply = earnings::compute(&body)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
