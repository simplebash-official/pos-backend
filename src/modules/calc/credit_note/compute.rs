use super::calc_credit_note_reply::CalcCreditNoteReply;
use super::calc_credit_note_request::CalcCreditNoteRequest;
use super::calc_refund_leg::CalcRefundLeg;
use super::calc_returned_line_reply::CalcReturnedLineReply;
use super::invoice_context::InvoiceContext;
use crate::core::calculations::{default_refund_legs, refund_amounts, returned_unit_price_cents};
use crate::core::error::{AppError, AppResult};

const MAX_LINES: usize = 500;

const MAX_CENTS: i64 = 10_000_000_000_000;

const MAX_QUANTITY: i64 = 1_000_000;

fn checked(label: &str, cents: i64, quantity: i64) -> AppResult<i64> {
    if !(0..=MAX_CENTS).contains(&cents) || !(0..=MAX_QUANTITY).contains(&quantity) {
        return Err(AppError::validation(format!("{label} is out of range")));
    }
    cents
        .checked_mul(quantity)
        .ok_or_else(|| AppError::validation(format!("{label} is too large")))
}

pub fn compute(
    req: &CalcCreditNoteRequest,
    context: Option<&InvoiceContext>,
    days_since_sale: Option<i64>,
    return_window_days: i64,
) -> AppResult<CalcCreditNoteReply> {
    if req.returned.len() > MAX_LINES || req.exchanged.len() > MAX_LINES {
        return Err(AppError::validation("There are too many lines"));
    }
    if (req.invoice_key.is_some() == req.no_receipt.is_some())
        || context.is_some() != req.invoice_key.is_some()
    {
        return Err(AppError::validation(
            "A return is either against an invoice or a no-receipt line, not both and not neither",
        ));
    }
    let no_receipt = req.no_receipt.is_some();

    let mut lines = Vec::with_capacity(req.returned.len());
    let mut return_subtotal = 0_i64;
    if let Some(context) = context {
        for returned in &req.returned {
            let item = context
                .invoice
                .items
                .get(returned.line_index)
                .ok_or_else(|| AppError::validation("A returned line is not on the invoice"))?;
            if returned.quantity > item.quantity - item.returned_quantity {
                return Err(AppError::validation(
                    "More units are being returned than are left to return",
                ));
            }
            let unit_cents =
                returned_unit_price_cents(item.total_cents, item.quantity, item.unit_price_cents);
            let refund_cents = checked("A returned line", unit_cents, returned.quantity)?;
            return_subtotal = return_subtotal
                .checked_add(refund_cents)
                .ok_or_else(|| AppError::validation("The return is too large"))?;
            lines.push(CalcReturnedLineReply {
                line_index: returned.line_index,
                unit_cents,
                refund_cents,
            });
        }
    } else if !req.returned.is_empty() {
        return Err(AppError::validation(
            "A no-receipt return has no invoice lines",
        ));
    }
    if let Some(line) = &req.no_receipt {
        return_subtotal = checked("The returned product", line.unit_price_cents, line.quantity)?;
    }

    let mut exchange_subtotal = 0_i64;
    for line in &req.exchanged {
        let total = checked("An exchange line", line.unit_price_cents, line.quantity)?;
        exchange_subtotal = exchange_subtotal
            .checked_add(total)
            .ok_or_else(|| AppError::validation("The exchange is too large"))?;
    }

    let net = return_subtotal - exchange_subtotal;
    let already_paid = context.map_or(0, |c| c.payments.iter().map(|(_, cents)| cents).sum());
    let (refund_cash, balance_reduction) = refund_amounts(net, already_paid, no_receipt);
    let refund_legs = match context {
        Some(c) => default_refund_legs(refund_cash, c.payments, &c.invoice.payment_method),
        None => Vec::new(),
    };
    Ok(CalcCreditNoteReply {
        lines,
        return_subtotal_cents: return_subtotal,
        exchange_subtotal_cents: exchange_subtotal,
        net_refund_cents: net,
        refund_cash_cents: refund_cash,
        balance_reduction_cents: balance_reduction,
        refund_legs: refund_legs
            .into_iter()
            .map(|(method, amount_cents)| CalcRefundLeg {
                method,
                amount_cents,
            })
            .collect(),
        is_refund_due: net > 0,
        is_customer_owing: net < 0,
        is_even_exchange: net == 0
            && (!req.returned.is_empty() || no_receipt || !req.exchanged.is_empty()),
        days_since_sale,
        return_window_days,
        past_return_window: days_since_sale.is_some_and(|days| days > return_window_days),
    })
}
