use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{
    calculations::{
        CartLine, LineDiscount, LineSource, cart_totals, line_totals, percentage_discount_cents,
    },
    error::{CalcError, CalcResult},
};

const MAX_LINES: usize = 200;
const MAX_CENTS: i64 = 10_000_000_000_000;
const MAX_QUANTITY: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum QuotationDiscountType {
    Percentage,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum QuotationDiscountScope {
    /// A fixed amount off each unit.
    PerUnit,
    /// A fixed amount off the whole line.
    Total,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcQuotationLineDiscount {
    #[serde(rename = "type")]
    pub kind: QuotationDiscountType,
    /// Only used for a fixed discount on a line of more than one unit; otherwise it is the whole line.
    pub scope: QuotationDiscountScope,
    /// A percentage (0-100) or, for a fixed discount, cents.
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcQuotationLine {
    /// The form line's id, echoed in its answer.
    pub key: String,
    pub name: String,
    pub unit_price_cents: i64,
    pub quantity: i64,
    pub discount: Option<CalcQuotationLineDiscount>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcQuotationOrderDiscount {
    #[serde(rename = "type")]
    pub kind: QuotationDiscountType,
    /// A percentage (0-100) or, for a fixed discount, cents.
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcQuotationRequest {
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub lines: Vec<CalcQuotationLine>,
    pub order_discount: Option<CalcQuotationOrderDiscount>,
    /// `YYYY-MM-DD`.
    pub valid_until: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcQuotationLineReply {
    pub key: String,
    pub gross_cents: i64,
    /// What this line's discount comes to; what the create request sends as `discountCents`.
    pub discount_cents: i64,
    pub total_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum QuotationBlockerCode {
    CustomerMissing,
    NoLines,
    NameMissing,
    QuantityInvalid,
    OrderDiscountNegative,
    OrderDiscountOver100,
    ValidUntilInvalid,
    ValidUntilInPast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct QuotationBlocker {
    pub code: QuotationBlockerCode,
    /// Zero-based line, for the line blockers.
    pub line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcQuotationReply {
    pub lines: Vec<CalcQuotationLineReply>,
    /// Price times quantity over all lines, before any discount.
    pub gross_subtotal_cents: i64,
    /// All the line discounts together.
    pub line_discounts_cents: i64,
    pub subtotal_cents: i64,
    pub discount_cents: i64,
    pub total_cents: i64,
    /// What stops the quotation being created, in the order to say it; empty means it can go ahead.
    pub blockers: Vec<QuotationBlocker>,
}

fn line_discount_cents(line: &CalcQuotationLine, gross: i64) -> i64 {
    let Some(discount) = &line.discount else {
        return 0;
    };
    if gross == 0 || discount.value <= 0.0 {
        return 0;
    }
    match discount.kind {
        QuotationDiscountType::Percentage => percentage_discount_cents(gross, discount.value),
        QuotationDiscountType::Fixed => {
            let cents = discount.value.round() as i64;
            let total = if discount.scope == QuotationDiscountScope::PerUnit && line.quantity > 1 {
                cents.saturating_mul(line.quantity)
            } else {
                cents
            };
            total.clamp(0, gross)
        }
    }
}

pub fn compute(req: &CalcQuotationRequest, today: NaiveDate) -> CalcResult<CalcQuotationReply> {
    if req.lines.len() > MAX_LINES {
        return Err(CalcError::validation("There are too many lines"));
    }
    for line in &req.lines {
        if !(0..=MAX_CENTS).contains(&line.unit_price_cents)
            || !(0..=MAX_QUANTITY).contains(&line.quantity)
        {
            return Err(CalcError::validation("An amount or count is out of range"));
        }
        if let Some(d) = &line.discount
            && !(d.value.is_finite() && (0.0..=1e13).contains(&d.value))
        {
            return Err(CalcError::validation("An amount or count is out of range"));
        }
    }
    if let Some(d) = &req.order_discount
        && !(d.value.is_finite() && d.value <= 1e13)
    {
        return Err(CalcError::validation("An amount or count is out of range"));
    }

    let mut cart = Vec::with_capacity(req.lines.len());
    let mut discounts = Vec::with_capacity(req.lines.len());
    for line in &req.lines {
        let gross = line_totals(line.unit_price_cents, line.quantity, 0)?.gross_cents;
        let cents = line_discount_cents(line, gross);
        discounts.push(cents);
        cart.push(CartLine {
            unit_price_cents: line.unit_price_cents,
            quantity: line.quantity,
            discount: LineDiscount::Fixed(cents),
            source: LineSource::Retail,
        });
    }
    let (kind, value) = match &req.order_discount {
        Some(d) if d.value > 0.0 => (
            Some(match d.kind {
                QuotationDiscountType::Percentage => "percentage",
                QuotationDiscountType::Fixed => "fixed",
            }),
            d.value,
        ),
        _ => (None, 0.0),
    };
    let totals = cart_totals(&cart, kind, value)?;

    let mut blockers = Vec::new();
    let mut block = |code, line| blockers.push(QuotationBlocker { code, line });
    if req.customer_key.as_deref().is_none_or(str::is_empty) && req.customer_name.trim().is_empty()
    {
        block(QuotationBlockerCode::CustomerMissing, None);
    }
    if req.lines.is_empty() {
        block(QuotationBlockerCode::NoLines, None);
    }
    for (index, line) in req.lines.iter().enumerate() {
        if line.name.trim().is_empty() {
            block(QuotationBlockerCode::NameMissing, Some(index));
        } else if line.quantity < 1 {
            block(QuotationBlockerCode::QuantityInvalid, Some(index));
        }
    }
    if let Some(d) = &req.order_discount {
        if d.value < 0.0 {
            block(QuotationBlockerCode::OrderDiscountNegative, None);
        } else if d.kind == QuotationDiscountType::Percentage && d.value > 100.0 {
            block(QuotationBlockerCode::OrderDiscountOver100, None);
        }
    }
    let valid_until = NaiveDate::parse_from_str(&req.valid_until, "%Y-%m-%d")
        .ok()
        .filter(|_| req.valid_until.len() == 10);
    match valid_until {
        None => block(QuotationBlockerCode::ValidUntilInvalid, None),
        Some(day) if day < today => block(QuotationBlockerCode::ValidUntilInPast, None),
        Some(_) => {}
    }

    Ok(CalcQuotationReply {
        lines: req
            .lines
            .iter()
            .zip(&totals.lines)
            .map(|(line, t)| CalcQuotationLineReply {
                key: line.key.clone(),
                gross_cents: t.gross_cents,
                discount_cents: t.discount_cents,
                total_cents: t.total_cents,
            })
            .collect(),
        gross_subtotal_cents: totals.lines.iter().map(|t| t.gross_cents).sum(),
        line_discounts_cents: totals.lines.iter().map(|t| t.discount_cents).sum(),
        subtotal_cents: totals.subtotal_cents,
        discount_cents: totals.order_discount_cents,
        total_cents: totals.total_cents,
        blockers,
    })
}
