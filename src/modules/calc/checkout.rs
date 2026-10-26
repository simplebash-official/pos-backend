// The checkout screen's live figures: every cart line, the totals, the order discount, the
// payment state (change, short by, split, credit) and the quick-tender buttons. Pure: the maths is `core::calculations`, the same code the sale is saved with.
//
// A box still empty is 0 or absent; nothing is defaulted on the way. Malformed or out-of-range
// input is a 400.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::core::{
    calculations::{
        CartLine, LineDiscount, LineSource, PaymentInput, TenderMethod, cart_totals, payment_state,
        quick_tender_suggestions,
    },
    error::{AppError, AppResult},
};

const MAX_LINES: usize = 500;
const MAX_LEGS: usize = 20;
/// Large enough for any real sale, small enough that no sum of 500 lines can overflow.
const MAX_CENTS: i64 = 10_000_000_000_000;
const MAX_QUANTITY: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CalcSource {
    Retail,
    Repair,
    Print,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CalcPaymentMethod {
    Cash,
    Card,
    Online,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CalcDiscountType {
    Percentage,
    Fixed,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCartLine {
    /// The client's id for this line, echoed back so a reply can be matched to its line even when
    /// it answers an earlier version of the cart.
    pub key: String,
    pub unit_price_cents: i64,
    pub quantity: i64,
    /// What was typed as this line's discount: a percentage of the line (0-100) or fixed cents.
    /// `None` for no discount.
    pub discount: Option<CalcDiscount>,
    pub source_type: CalcSource,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcDiscount {
    #[serde(rename = "type")]
    pub kind: CalcDiscountType,
    /// A percentage (0-100) or, for a fixed discount, cents.
    pub value: f64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCheckoutPayment {
    pub method: CalcPaymentMethod,
    /// Cash handed over; 0 while nothing is typed.
    pub tendered_cents: i64,
    /// The amount of each split leg.
    pub split_legs_cents: Vec<i64>,
    /// What the customer owed before this sale; 0 for a walk-in.
    pub customer_balance_cents: i64,
    pub is_credit: bool,
    /// For a credit sale: what is paid up front.
    pub credit_deposit_cents: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCheckoutRequest {
    pub items: Vec<CalcCartLine>,
    pub order_discount: Option<CalcDiscount>,
    pub payment: CalcCheckoutPayment,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcLineReply {
    /// The `key` of the request line this answers.
    pub key: String,
    pub gross_cents: i64,
    pub discount_cents: i64,
    pub total_cents: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSourceBreakdown {
    pub retail_cents: i64,
    pub repairs_cents: i64,
    pub print_cents: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcPaymentReply {
    pub effective_tendered_cents: i64,
    pub change_due_cents: i64,
    pub short_by_cents: i64,
    pub is_cash_short: bool,
    pub is_card_short: bool,
    pub is_fully_paid: bool,
    pub split_allocated_cents: i64,
    pub split_remaining_cents: i64,
    pub is_split_valid: bool,
    pub new_customer_balance_cents: i64,
    pub credit_paid_now_cents: i64,
    pub credit_remainder_cents: i64,
    /// The `amountReceivedCents` to send with the sale.
    pub amount_received_cents: i64,
    /// The most a credit sale can take up front: always at least one cent stays on account.
    pub max_credit_deposit_cents: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCheckoutReply {
    /// One per request line, in order.
    pub items: Vec<CalcLineReply>,
    pub item_count: usize,
    pub total_unit_count: i64,
    pub subtotal_cents: i64,
    pub order_discount_cents: i64,
    pub total_cents: i64,
    pub source_breakdown: CalcSourceBreakdown,
    pub payment: CalcPaymentReply,
    /// Cash amounts to offer as quick buttons, smallest first.
    pub quick_tender_cents: Vec<i64>,
}

fn in_range(cents: i64) -> bool {
    (0..=MAX_CENTS).contains(&cents)
}

fn discount_in_range(discount: &CalcDiscount) -> bool {
    discount.value.is_finite() && (0.0..=1e13).contains(&discount.value)
}

pub fn compute_checkout(req: &CalcCheckoutRequest) -> AppResult<CalcCheckoutReply> {
    let out_of_range = || AppError::validation("An amount or count is out of range");
    if req.items.len() > MAX_LINES || req.payment.split_legs_cents.len() > MAX_LEGS {
        return Err(AppError::validation("There are too many lines or payments"));
    }
    for item in &req.items {
        if !in_range(item.unit_price_cents) || !(0..=MAX_QUANTITY).contains(&item.quantity) {
            return Err(out_of_range());
        }
        if let Some(discount) = &item.discount
            && !discount_in_range(discount)
        {
            return Err(out_of_range());
        }
    }
    let payment = &req.payment;
    let legs_in_range = payment.split_legs_cents.iter().all(|&c| in_range(c));
    if !in_range(payment.tendered_cents)
        || !in_range(payment.credit_deposit_cents)
        || !(-MAX_CENTS..=MAX_CENTS).contains(&payment.customer_balance_cents)
        || !legs_in_range
    {
        return Err(out_of_range());
    }
    if let Some(discount) = &req.order_discount
        && !discount_in_range(discount)
    {
        return Err(out_of_range());
    }

    let lines: Vec<CartLine> = req
        .items
        .iter()
        .map(|item| CartLine {
            unit_price_cents: item.unit_price_cents,
            quantity: item.quantity,
            discount: match &item.discount {
                None => LineDiscount::None,
                Some(d) => match d.kind {
                    CalcDiscountType::Percentage => LineDiscount::Percentage(d.value),
                    CalcDiscountType::Fixed => LineDiscount::Fixed(d.value.round() as i64),
                },
            },
            source: match item.source_type {
                CalcSource::Retail => LineSource::Retail,
                CalcSource::Repair => LineSource::Repair,
                CalcSource::Print => LineSource::Print,
            },
        })
        .collect();
    let (kind, value) = match &req.order_discount {
        Some(d) => (
            Some(match d.kind {
                CalcDiscountType::Percentage => "percentage",
                CalcDiscountType::Fixed => "fixed",
            }),
            d.value,
        ),
        None => (None, 0.0),
    };
    let totals = cart_totals(&lines, kind, value)?;
    let state = payment_state(&PaymentInput {
        total_cents: totals.total_cents,
        tendered_cents: payment.tendered_cents,
        method: match payment.method {
            CalcPaymentMethod::Cash => TenderMethod::Cash,
            CalcPaymentMethod::Card => TenderMethod::Card,
            CalcPaymentMethod::Online => TenderMethod::Online,
            CalcPaymentMethod::Split => TenderMethod::Split,
        },
        split_legs_cents: &payment.split_legs_cents,
        customer_balance_cents: payment.customer_balance_cents,
        is_credit: payment.is_credit,
        credit_deposit_cents: payment.credit_deposit_cents,
    })?;

    Ok(CalcCheckoutReply {
        items: totals
            .lines
            .iter()
            .zip(&req.items)
            .map(|(l, item)| CalcLineReply {
                key: item.key.clone(),
                gross_cents: l.gross_cents,
                discount_cents: l.discount_cents,
                total_cents: l.total_cents,
            })
            .collect(),
        item_count: totals.lines.len(),
        total_unit_count: totals.unit_count,
        subtotal_cents: totals.subtotal_cents,
        order_discount_cents: totals.order_discount_cents,
        total_cents: totals.total_cents,
        source_breakdown: CalcSourceBreakdown {
            retail_cents: totals.retail_cents,
            repairs_cents: totals.repairs_cents,
            print_cents: totals.print_cents,
        },
        payment: CalcPaymentReply {
            effective_tendered_cents: state.effective_tendered_cents,
            change_due_cents: state.change_due_cents,
            short_by_cents: state.short_by_cents,
            is_cash_short: state.is_cash_short,
            is_card_short: state.is_card_short,
            is_fully_paid: state.is_fully_paid,
            split_allocated_cents: state.split_allocated_cents,
            split_remaining_cents: state.split_remaining_cents,
            is_split_valid: state.is_split_valid,
            new_customer_balance_cents: state.new_customer_balance_cents,
            credit_paid_now_cents: state.credit_paid_now_cents,
            credit_remainder_cents: state.credit_remainder_cents,
            amount_received_cents: state.amount_received_cents,
            max_credit_deposit_cents: (totals.total_cents - 1).max(0),
        },
        quick_tender_cents: quick_tender_suggestions(totals.total_cents),
    })
}
