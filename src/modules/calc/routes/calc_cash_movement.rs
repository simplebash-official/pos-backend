use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::modules::calc::cash::{CalcCashMovementReply, CalcCashMovementRequest, MovementContext};

use crate::modules::calc::cash;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/cash-movement", tag = modules::CALC, request_body = CalcCashMovementRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What the payment does to the drawer, whether a PIN is needed and what still stops it being saved", body = ApiResponse<CalcCashMovementReply>),
        (status = 400, description = "An amount is out of range, or the category does not belong to this kind of payment", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Shift or category not found", body = ErrorResponse),
    )
)]
/// Same access as recording the movement (`cash_movements:create`, plus the managed one for rent,
/// staff and supplier payments). Reads the drawer, the category, the shop limit and the month's bills.
pub(super) async fn calc_cash_movement(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcCashMovementRequest>,
) -> AppResult<Json<ApiResponse<CalcCashMovementReply>>> {
    use crate::domain::recurring_expenses::PeriodStatusQuery;
    use crate::domain::sessions::CashMovementType;
    use crate::modules::{cash_categories, cash_settings, recurring_expenses, sessions};

    user.require_permission(perm::CASH_MOVEMENTS_CREATE)?;
    if sessions::service::requires_managed_permission(body.movement_type) {
        user.require_permission(perm::CASH_MOVEMENTS_CREATE_MANAGED)?;
    }
    let db = &state.db;
    let cash_in_drawer_cents =
        sessions::service::cash_in_drawer(db, body.session_key.as_deref(), &body.terminal_id)
            .await?;
    let category_limit_cents = match &body.category_key {
        Some(key) => {
            cash_categories::service::category_for_movement(db, key, body.movement_type)
                .await?
                .requires_approval_over_cents
        }
        None => None,
    };
    let shop_limit_cents = cash_settings::service::approval_threshold_cents(db).await?;
    let period_already_paid = match (&body.category_key, &body.payee_key, &body.period) {
        (Some(category_key), Some(payee_key), Some(period))
            if body.movement_type == CashMovementType::RentAndBills =>
        {
            recurring_expenses::service::period_status(
                db,
                PeriodStatusQuery {
                    category_key: category_key.clone(),
                    payee_key: Some(payee_key.clone()),
                    period: period.clone(),
                },
            )
            .await?
            .already_paid
        }
        _ => false,
    };
    let reply = cash::compute_movement(
        &body,
        &MovementContext {
            cash_in_drawer_cents,
            category_limit_cents,
            shop_limit_cents,
            period_already_paid,
        },
    )?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
