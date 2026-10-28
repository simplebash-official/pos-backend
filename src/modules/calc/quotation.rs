// The quotation form's live figures: what each line's discount comes to, each line's total, the
// subtotal, the order discount and the total, plus what still stops it being created. Pure; the
// maths is `pos_calc_core::quotation`.

use crate::core::error::AppResult;
use chrono::NaiveDate;

pub use pos_calc_core::quotation::{
    CalcQuotationLine, CalcQuotationLineDiscount, CalcQuotationLineReply,
    CalcQuotationOrderDiscount, CalcQuotationReply, CalcQuotationRequest, QuotationBlocker,
    QuotationBlockerCode, QuotationDiscountScope, QuotationDiscountType,
};

pub fn compute(req: &CalcQuotationRequest, today: NaiveDate) -> AppResult<CalcQuotationReply> {
    Ok(pos_calc_core::quotation::compute(req, today)?)
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
