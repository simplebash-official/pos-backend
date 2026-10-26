// The quotation form's live figures: what each line's discount comes to, each line's total, the
// subtotal, the order discount and the total, plus what still stops it being created. Pure; the
// maths is `core::calculations`, the same code the quotation is created with. The create request
// still carries each line's discount in cents (the server re-checks it), taken from this answer.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::core::{
    calculations::{
        CartLine, LineDiscount, LineSource, cart_totals, line_totals, percentage_discount_cents,
    },
    error::{AppError, AppResult},
};

const MAX_LINES: usize = 200;
const MAX_CENTS: i64 = 10_000_000_000_000;
const MAX_QUANTITY: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuotationDiscountType {
    Percentage,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuotationDiscountScope {
    /// A fixed amount off each unit.
    PerUnit,
    /// A fixed amount off the whole line.
    Total,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcQuotationLineDiscount {
    #[serde(rename = "type")]
    pub kind: QuotationDiscountType,
    /// Only used for a fixed discount on a line of more than one unit; otherwise it is the whole line.
    pub scope: QuotationDiscountScope,
    /// A percentage (0-100) or, for a fixed discount, cents.
    pub value: f64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcQuotationLine {
    /// The form line's id, echoed in its answer.
    pub key: String,
    pub name: String,
    pub unit_price_cents: i64,
    pub quantity: i64,
    pub discount: Option<CalcQuotationLineDiscount>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcQuotationOrderDiscount {
    #[serde(rename = "type")]
    pub kind: QuotationDiscountType,
    /// A percentage (0-100) or, for a fixed discount, cents.
    pub value: f64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcQuotationRequest {
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub lines: Vec<CalcQuotationLine>,
    pub order_discount: Option<CalcQuotationOrderDiscount>,
    /// `YYYY-MM-DD`.
    pub valid_until: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcQuotationLineReply {
    pub key: String,
    pub gross_cents: i64,
    /// What this line's discount comes to; what the create request sends as `discountCents`.
    pub discount_cents: i64,
    pub total_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
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

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuotationBlocker {
    pub code: QuotationBlockerCode,
    /// Zero-based line, for the line blockers.
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
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

pub fn compute(req: &CalcQuotationRequest, today: NaiveDate) -> AppResult<CalcQuotationReply> {
    if req.lines.len() > MAX_LINES {
        return Err(AppError::validation("There are too many lines"));
    }
    for line in &req.lines {
        if !(0..=MAX_CENTS).contains(&line.unit_price_cents)
            || !(0..=MAX_QUANTITY).contains(&line.quantity)
        {
            return Err(AppError::validation("An amount or count is out of range"));
        }
        if let Some(d) = &line.discount
            && !(d.value.is_finite() && (0.0..=1e13).contains(&d.value))
        {
            return Err(AppError::validation("An amount or count is out of range"));
        }
    }
    if let Some(d) = &req.order_discount
        && !(d.value.is_finite() && d.value <= 1e13)
    {
        return Err(AppError::validation("An amount or count is out of range"));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
    }

    fn line(
        key: &str,
        unit: i64,
        quantity: i64,
        discount: Option<CalcQuotationLineDiscount>,
    ) -> CalcQuotationLine {
        CalcQuotationLine {
            key: key.into(),
            name: format!("Item {key}"),
            unit_price_cents: unit,
            quantity,
            discount,
        }
    }

    fn discount(
        kind: QuotationDiscountType,
        scope: QuotationDiscountScope,
        value: f64,
    ) -> Option<CalcQuotationLineDiscount> {
        Some(CalcQuotationLineDiscount { kind, scope, value })
    }

    fn request(lines: Vec<CalcQuotationLine>) -> CalcQuotationRequest {
        CalcQuotationRequest {
            customer_key: Some("cus_1".into()),
            customer_name: String::new(),
            lines,
            order_discount: None,
            valid_until: "2026-11-25".into(),
        }
    }

    #[test]
    fn a_percentage_a_per_unit_and_a_whole_line_discount() {
        use QuotationDiscountScope::*;
        use QuotationDiscountType::*;
        let reply = compute(
            &request(vec![
                line("a", 650, 1, discount(Percentage, Total, 10.0)),
                line("b", 1000, 3, discount(Fixed, PerUnit, 150.0)),
                line("c", 1000, 3, discount(Fixed, Total, 150.0)),
                line("d", 500, 1, discount(Fixed, Total, 800.0)),
            ]),
            today(),
        )
        .unwrap();
        let discounts: Vec<i64> = reply.lines.iter().map(|l| l.discount_cents).collect();
        assert_eq!(discounts, vec![65, 450, 150, 500]);
        let totals: Vec<i64> = reply.lines.iter().map(|l| l.total_cents).collect();
        assert_eq!(totals, vec![585, 2550, 2850, 0]);
        assert_eq!(reply.subtotal_cents, 585 + 2550 + 2850);
        assert_eq!(reply.gross_subtotal_cents, 650 + 3000 + 3000 + 500);
        assert_eq!(reply.line_discounts_cents, 65 + 450 + 150 + 500);
        assert!(reply.blockers.is_empty(), "{:?}", reply.blockers);
    }

    #[test]
    fn a_per_unit_discount_on_one_unit_is_the_whole_line_and_the_order_discount_applies_after() {
        use QuotationDiscountScope::*;
        use QuotationDiscountType::*;
        let mut req = request(vec![line("a", 1000, 1, discount(Fixed, PerUnit, 200.0))]);
        req.order_discount = Some(CalcQuotationOrderDiscount {
            kind: Percentage,
            value: 10.0,
        });
        let reply = compute(&req, today()).unwrap();
        assert_eq!(reply.lines[0].discount_cents, 200);
        assert_eq!(
            (
                reply.subtotal_cents,
                reply.discount_cents,
                reply.total_cents
            ),
            (800, 80, 720)
        );
    }

    #[test]
    fn blockers_say_what_is_missing_in_order() {
        let mut req = request(vec![line("a", 100, 0, None)]);
        req.customer_key = None;
        req.lines[0].name = "  ".into();
        req.valid_until = "2026-10-01".into();
        req.order_discount = Some(CalcQuotationOrderDiscount {
            kind: QuotationDiscountType::Percentage,
            value: 150.0,
        });
        let codes: Vec<_> = compute(&req, today())
            .unwrap()
            .blockers
            .iter()
            .map(|b| b.code)
            .collect();
        assert_eq!(
            codes,
            vec![
                QuotationBlockerCode::CustomerMissing,
                QuotationBlockerCode::NameMissing,
                QuotationBlockerCode::OrderDiscountOver100,
                QuotationBlockerCode::ValidUntilInPast,
            ]
        );
        let mut empty = request(vec![]);
        empty.valid_until = "2026-02-30".into();
        let codes: Vec<_> = compute(&empty, today())
            .unwrap()
            .blockers
            .iter()
            .map(|b| b.code)
            .collect();
        assert_eq!(
            codes,
            vec![
                QuotationBlockerCode::NoLines,
                QuotationBlockerCode::ValidUntilInvalid
            ]
        );
    }

    #[test]
    fn today_is_still_valid_and_out_of_range_input_is_refused() {
        let mut req = request(vec![line("a", 100, 1, None)]);
        req.valid_until = "2026-10-26".into();
        assert!(compute(&req, today()).unwrap().blockers.is_empty());
        req.lines[0].unit_price_cents = -1;
        assert!(compute(&req, today()).is_err());
    }
}
