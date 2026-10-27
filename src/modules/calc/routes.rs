// HTTP layer for the live calculations: each handler checks the same permission as the write it
// previews, makes one call into the pure engine and wraps the reply. Nothing is stored, so none of
// these count as writes (no idempotency key, no sync wake-up; see `app.rs`).

use axum::{Json, extract::State};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        constants::permissions as perm,
        error::AppResult,
        middleware::auth::{AdminUser, CurrentUser},
        response::{ApiResponse, ErrorResponse},
    },
    modules::calc::{
        cash::{
            self, CalcCashCountReply, CalcCashCountRequest, CalcCashMovementReply,
            CalcCashMovementRequest, CalcShiftCountReply, CalcShiftCountRequest, MovementContext,
        },
        checkout::{self, CalcCheckoutReply, CalcCheckoutRequest},
        credit_note::{self, CalcCreditNoteReply, CalcCreditNoteRequest, InvoiceContext},
        earnings::{self, CalcEarningsReply, CalcEarningsRequest},
        invoice_balance::{self, CalcInvoiceBalanceReply, CalcInvoiceBalanceRequest},
        quotation::{self, CalcQuotationReply, CalcQuotationRequest},
        receipt::{self, CalcReceiptReply, CalcReceiptRequest},
        settlement::{self, CalcSettlementReply, CalcSettlementRequest, OpenBill},
        shares::{self, CalcSharesReply, CalcSharesRequest},
        shift_totals::{self, CalcShiftTotalsReply, CalcShiftTotalsRequest, ShiftFigures},
    },
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(calc_receipt))
        .routes(routes!(calc_checkout))
        .routes(routes!(calc_credit_note))
        .routes(routes!(calc_quotation))
        .routes(routes!(calc_cash_movement))
        .routes(routes!(calc_cash_count))
        .routes(routes!(calc_shift_count))
        .routes(routes!(calc_earnings))
        .routes(routes!(calc_shares))
        .routes(routes!(calc_shift_totals))
        .routes(routes!(calc_settlement))
        .routes(routes!(calc_invoice_balance))
}

// ============================================================================
// Repair and print job earnings
// ============================================================================

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
async fn calc_earnings(
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

// ============================================================================
// Shares
// ============================================================================

#[utoipa::path(post, path = "/shares", tag = modules::CALC, request_body = CalcSharesRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Each part's share of its whole, for progress and split bars", body = ApiResponse<CalcSharesReply>),
        (status = 400, description = "An amount is out of range, or there are too many parts", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
/// Any signed-in user: it only turns figures the screen already shows into bar lengths.
async fn calc_shares(
    _user: CurrentUser,
    Json(body): Json<CalcSharesRequest>,
) -> AppResult<Json<ApiResponse<CalcSharesReply>>> {
    let reply = shares::compute(&body)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}

#[utoipa::path(post, path = "/shift-totals", tag = modules::CALC, request_body = CalcShiftTotalsRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "How many of the listed shifts are running, their cash sales and the closed ones' net difference", body = ApiResponse<CalcShiftTotalsReply>),
        (status = 400, description = "Too many shifts", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "A shift was not found", body = ErrorResponse),
    )
)]
/// Needs `register_sessions:view`, like the shift list it totals. Reads the shifts by key.
async fn calc_shift_totals(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcShiftTotalsRequest>,
) -> AppResult<Json<ApiResponse<CalcShiftTotalsReply>>> {
    use crate::{core::constants::codes, modules::sessions};

    user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    shift_totals::check(&body)?;
    let mut shifts = Vec::with_capacity(body.session_keys.len());
    for key in &body.session_keys {
        let session = sessions::repository::sessions::find_session_by_key(&state.db, key)
            .await?
            .ok_or_else(|| {
                crate::core::error::AppError::not_found_with_code(
                    "Register session not found",
                    codes::SESSION_NOT_FOUND,
                )
            })?;
        shifts.push(ShiftFigures {
            status: session.status,
            cash_sales_cents: session.total_cash_sales_cents,
            cash_variance_cents: session.cash_variance_cents,
        });
    }
    Ok(Json(ApiResponse::success(
        shift_totals::compute(&shifts),
        "Calculated",
    )))
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

// ============================================================================
// Checkout
// ============================================================================

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
async fn calc_checkout(
    user: CurrentUser,
    Json(body): Json<CalcCheckoutRequest>,
) -> AppResult<Json<ApiResponse<CalcCheckoutReply>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let reply = checkout::compute_checkout(&body)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}

#[utoipa::path(post, path = "/credit-note", tag = modules::CALC, request_body = CalcCreditNoteRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What each returned unit is worth back, the net, the cash refund, where it goes and whether the return window has passed", body = ApiResponse<CalcCreditNoteReply>),
        (status = 400, description = "A line is not on the invoice, too many units are returned, or an amount is out of range", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
    )
)]
/// Same access as creating a credit note (`billing:write`). Reads the invoice and its payments;
/// writes nothing.
async fn calc_credit_note(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcCreditNoteRequest>,
) -> AppResult<Json<ApiResponse<CalcCreditNoteReply>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let window = state.config.return_window_days;
    let reply = match &body.invoice_key {
        Some(key) => {
            let invoice = crate::modules::billing::service::get_invoice(&state.db, key).await?;
            let payments = crate::modules::billing::service::credit_notes::invoice_payment_legs(
                &state.db,
                &invoice.key,
            )
            .await?;
            let days = (chrono::Utc::now() - invoice.created_at).num_days();
            let context = InvoiceContext {
                invoice: &invoice,
                payments: &payments,
            };
            credit_note::compute(&body, Some(&context), Some(days), window)?
        }
        None => credit_note::compute(&body, None, None, window)?,
    };
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}

// ============================================================================
// Quotations
// ============================================================================

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
async fn calc_quotation(
    user: CurrentUser,
    Json(body): Json<CalcQuotationRequest>,
) -> AppResult<Json<ApiResponse<CalcQuotationReply>>> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let reply = quotation::compute(&body, chrono::Utc::now().date_naive())?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}

// ============================================================================
// Cash
// ============================================================================

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
async fn calc_cash_movement(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcCashMovementRequest>,
) -> AppResult<Json<ApiResponse<CalcCashMovementReply>>> {
    use crate::domain::{recurring_expenses::PeriodStatusQuery, sessions::CashMovementType};
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

#[utoipa::path(post, path = "/cash-count", tag = modules::CALC, request_body = CalcCashCountRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Every denomination with its subtotal, and the total", body = ApiResponse<CalcCashCountReply>),
        (status = 400, description = "Not a note or coin, or a count is out of range", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Needs `register_sessions:view`, like the screens that count cash or show a saved count.
async fn calc_cash_count(
    user: CurrentUser,
    Json(body): Json<CalcCashCountRequest>,
) -> AppResult<Json<ApiResponse<CalcCashCountReply>>> {
    user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let reply = cash::compute_count(&body)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}

#[utoipa::path(post, path = "/shift-count", tag = modules::CALC, request_body = CalcShiftCountRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What the drawer should hold, the difference, and whether a reason or PIN is needed", body = ApiResponse<CalcShiftCountReply>),
        (status = 400, description = "Both or neither of shift and terminal given, or an amount is out of range", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Shift not found", body = ErrorResponse),
    )
)]
/// Same access as opening or closing a shift (`register_sessions:manage`). Reads the shift or drawer.
async fn calc_shift_count(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcShiftCountRequest>,
) -> AppResult<Json<ApiResponse<CalcShiftCountReply>>> {
    use crate::{core::constants::codes, modules::sessions};

    user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let (expected, opening_float) = match (&body.session_key, &body.terminal_id) {
        (Some(key), None) => {
            let session = sessions::repository::sessions::find_session_by_key(&state.db, key)
                .await?
                .ok_or_else(|| {
                    crate::core::error::AppError::not_found_with_code(
                        "Register session not found",
                        codes::SESSION_NOT_FOUND,
                    )
                })?;
            (
                session.calculate_expected_cash(),
                Some(session.opening_float_cents),
            )
        }
        (None, Some(terminal_id)) => {
            let drawer =
                sessions::repository::drawer::get_drawer_state(&state.db, terminal_id).await?;
            (
                sessions::service::expected_opening_float(drawer.as_ref(), body.counted_cents),
                None,
            )
        }
        _ => {
            return Err(crate::core::error::AppError::validation(
                "Give either the shift being closed or the terminal being opened",
            ));
        }
    };
    let reply = cash::compute_shift_count(&body, expected, opening_float)?;
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}

// ============================================================================
// Supplier payment against bills
// ============================================================================

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
async fn calc_settlement(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcSettlementRequest>,
) -> AppResult<Json<ApiResponse<CalcSettlementReply>>> {
    use crate::{domain::purchase_receipts::ReceiptListQuery, modules::purchases};

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

// ============================================================================
// Invoice balance
// ============================================================================

#[utoipa::path(post, path = "/invoice-balance", tag = modules::CALC, request_body = CalcInvoiceBalanceRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What has been paid on the invoice and what is still owed", body = ApiResponse<CalcInvoiceBalanceReply>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
    )
)]
/// Any signed-in user, like reading the invoice's payments. Reads the invoice and its payments.
async fn calc_invoice_balance(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcInvoiceBalanceRequest>,
) -> AppResult<Json<ApiResponse<CalcInvoiceBalanceReply>>> {
    use crate::modules::billing::service;

    let invoice = service::get_invoice(&state.db, &body.invoice_key).await?;
    let payments = service::payments::list_payments(&state.db, &invoice.key).await?;
    let reply = invoice_balance::compute(&invoice, &payments);
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
