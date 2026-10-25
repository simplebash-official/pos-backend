// HTTP layer for the live calculations: each handler checks the same permission as the write it
// previews, makes one call into the pure engine and wraps the reply. Nothing is stored, so none of
// these count as writes (no idempotency key, no sync wake-up; see `app.rs`).

use axum::Json;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        middleware::auth::AdminUser,
        response::{ApiResponse, ErrorResponse},
    },
    modules::calc::receipt::{self, CalcReceiptReply, CalcReceiptRequest},
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(calc_receipt))
}

// ============================================================================
// Receive Stock
// ============================================================================

#[utoipa::path(post, path = "/receipt", tag = modules::CALC, request_body = CalcReceiptRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Every figure for the form as typed so far, and what still stops Save", body = ApiResponse<CalcReceiptReply>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 400, description = "An amount or count is out of range, or there are too many lines", body = ErrorResponse),
    )
)]
/// Same access as recording the receipt, so cost figures stay admin-only.
async fn calc_receipt(
    _admin: AdminUser,
    Json(body): Json<CalcReceiptRequest>,
) -> AppResult<Json<ApiResponse<CalcReceiptReply>>> {
    let reply = receipt::compute(&body)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
