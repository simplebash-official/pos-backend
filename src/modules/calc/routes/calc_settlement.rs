use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::calc::settlement::{CalcSettlementReply, CalcSettlementRequest, OpenBill};

use crate::modules::calc::settlement;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/settlement", tag = modules::CALC, request_body = CalcSettlementRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What the supplier's open bills owe, the payment spread over them oldest first, and what is wrong with a split made by hand", body = ApiResponse<CalcSettlementReply>),
        (status = 400, description = "An amount is out of range", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Needs `cash_movements:create:managed` (the cash form's supplier payment) or `expenses:manage`
/// (a bank, card or online payment). Reads the supplier's open bills.
pub(super) async fn calc_settlement(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcSettlementRequest>,
) -> AppResult<Json<ApiResponse<CalcSettlementReply>>> {
    use crate::domain::purchase_receipts::ReceiptListQuery;
    use crate::modules::purchases;

    user.require_any_permission(&[perm::CASH_MOVEMENTS_CREATE_MANAGED, perm::EXPENSES_MANAGE])?;
    let open = purchases::service::receipt::list_receipts(
        &state.db,
        ReceiptListQuery {
            supplier_key: Some(body.supplier_key.clone()),
            open: Some(true),
            page: None,
            limit: Some(500),
        },
    )
    .await?
    .items
    .into_iter()
    .map(|receipt| OpenBill {
        key: receipt.key,
        due_cents: receipt.due_cents,
    })
    .collect::<Vec<_>>();
    let reply = settlement::compute(&body, &open)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
