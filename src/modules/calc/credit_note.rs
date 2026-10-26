// The Return / Credit note screen's live figures: what each returned unit is worth back, the
// return and exchange subtotals, the net, how much of it goes back as cash (capped at what was paid
// on the invoice), where that cash goes, and whether the return window has passed. The invoice is
// read; nothing is written. The maths is `core::calculations`, the same code the credit note is
// created with, so the refund shown is the refund issued.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    core::{
        calculations::{default_refund_legs, refund_amounts, returned_unit_price_cents},
        error::{AppError, AppResult},
    },
    domain::billing::Invoice,
};

const MAX_LINES: usize = 500;
const MAX_CENTS: i64 = 10_000_000_000_000;
const MAX_QUANTITY: i64 = 1_000_000;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcReturnedLine {
    /// Position of the line in the invoice's items.
    pub line_index: usize,
    /// Units being returned now (one for a serial-tracked unit).
    pub quantity: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcNoReceiptLine {
    pub unit_price_cents: i64,
    pub quantity: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcExchangeLine {
    pub unit_price_cents: i64,
    pub quantity: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCreditNoteRequest {
    /// The invoice the return is against; absent (with `noReceipt`) for a return with no receipt.
    pub invoice_key: Option<String>,
    pub returned: Vec<CalcReturnedLine>,
    pub no_receipt: Option<CalcNoReceiptLine>,
    pub exchanged: Vec<CalcExchangeLine>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcReturnedLineReply {
    pub line_index: usize,
    /// What one unit of this line is worth back.
    pub unit_cents: i64,
    pub refund_cents: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcRefundLeg {
    pub method: String,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCreditNoteReply {
    pub lines: Vec<CalcReturnedLineReply>,
    pub return_subtotal_cents: i64,
    pub exchange_subtotal_cents: i64,
    /// Positive: the customer is owed this; negative: the customer owes it.
    pub net_refund_cents: i64,
    /// The part of the net that goes back as cash (capped at what was paid on the invoice).
    pub refund_cash_cents: i64,
    /// The part that only reduces what the customer owes.
    pub balance_reduction_cents: i64,
    /// Where the cash goes by default: in proportion to the invoice's payments when it was paid in
    /// several, otherwise one leg.
    pub refund_legs: Vec<CalcRefundLeg>,
    pub is_refund_due: bool,
    pub is_customer_owing: bool,
    pub is_even_exchange: bool,
    /// Whole days since the sale; absent for a return with no receipt.
    pub days_since_sale: Option<i64>,
    pub return_window_days: i64,
    /// True when the sale is older than the return window, which needs an administrator's approval.
    pub past_return_window: bool,
}

/// What the preview needs from the database, read by the route.
pub struct InvoiceContext<'a> {
    pub invoice: &'a Invoice,
    /// What the invoice has been paid so far, one entry per payment (method, cents).
    pub payments: &'a [(String, i64)],
}

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
