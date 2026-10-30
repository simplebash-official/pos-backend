use super::calc_cash_count;
use super::calc_cash_movement;
use super::calc_checkout;
use super::calc_credit_note;
use super::calc_earnings;
use super::calc_invoice_balance;
use super::calc_quotation;
use super::calc_receipt;
use super::calc_settlement;
use super::calc_shares;
use super::calc_shift_count;
use super::calc_shift_totals;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(calc_receipt::calc_receipt))
        .routes(routes!(calc_checkout::calc_checkout))
        .routes(routes!(calc_credit_note::calc_credit_note))
        .routes(routes!(calc_quotation::calc_quotation))
        .routes(routes!(calc_cash_movement::calc_cash_movement))
        .routes(routes!(calc_cash_count::calc_cash_count))
        .routes(routes!(calc_shift_count::calc_shift_count))
        .routes(routes!(calc_earnings::calc_earnings))
        .routes(routes!(calc_shares::calc_shares))
        .routes(routes!(calc_shift_totals::calc_shift_totals))
        .routes(routes!(calc_settlement::calc_settlement))
        .routes(routes!(calc_invoice_balance::calc_invoice_balance))
}
