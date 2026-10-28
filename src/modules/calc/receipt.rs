// The Receive Stock form's live figures. Pure: no I/O. The money maths is `pos_calc_core::receipt`.

use crate::core::error::AppResult;

pub use pos_calc_core::receipt::{
    Allocation, BilledBy, Blocker, BlockerCode, CalcCharge, CalcChargesPayment, CalcLine,
    CalcMargin, CalcPayment, CalcReceiptLineReply, CalcReceiptReply, CalcReceiptRequest,
    CalcReceiptSummary, ChargeKind, CostMode, PaymentMode, PaymentSource, PricingMode,
};

pub const MAX_CENTS: i64 = 10_000_000_000_000;
pub const MAX_QUANTITY: i64 = 1_000_000_000;

pub type CalcLineReply = CalcReceiptLineReply;

pub fn compute(request: &CalcReceiptRequest) -> AppResult<CalcReceiptReply> {
    Ok(pos_calc_core::receipt::compute(request)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(quantity: i64, unit_cents: i64) -> CalcLine {
        CalcLine {
            product_key: Some(format!("prod_{quantity}_{unit_cents}")),
            serialized: false,
            quantity: Some(quantity),
            cost_mode: CostMode::Unit,
            cost_cents: unit_cents,
            pricing_mode: PricingMode::Price,
            selling_price_cents: 1,
            margin_tenths: None,
            step_cents: None,
            serial_numbers: vec![],
        }
    }

    fn charge(kind: ChargeKind, cents: i64, by: BilledBy, how: Option<Allocation>) -> CalcCharge {
        CalcCharge {
            kind: Some(kind),
            amount_cents: cents,
            billed_by: Some(by),
            allocation: how,
            description: String::new(),
        }
    }

    fn request(lines: Vec<CalcLine>, charges: Vec<CalcCharge>) -> CalcReceiptRequest {
        CalcReceiptRequest {
            supplier_key: Some("sup_1".into()),
            reference_no: String::new(),
            date: "2026-10-26".into(),
            lines,
            charges,
            charges_payment: CalcChargesPayment {
                source: Some(PaymentSource::Drawer),
                paid_on: String::new(),
            },
            payment: CalcPayment {
                mode: PaymentMode::Full,
                source: Some(PaymentSource::Bank),
                part_cents: 0,
                paid_on: "2026-10-26".into(),
                category_key: Some("ccat_1".into()),
            },
        }
    }

    #[test]
    fn one_line_with_a_courier_charge_and_a_margin() {
        let mut one = line(10, 5_000);
        one.pricing_mode = PricingMode::Margin;
        one.margin_tenths = Some(250);
        one.step_cents = Some(100);
        let req = request(
            vec![one],
            vec![charge(
                ChargeKind::Shipping,
                3_000,
                BilledBy::ThirdParty,
                None,
            )],
        );
        let reply = compute(&req).unwrap();
        let l = &reply.lines[0];
        assert_eq!(l.goods_cents, 50_000);
        assert_eq!(l.landed_cents, 53_000);
        assert_eq!(l.unit_cost_cents, 5_300);
        assert_eq!(l.selling_price_cents, 7_100);
        assert_eq!(l.price_without_rounding_cents, Some(7_067));
        assert!(l.price_rounded_up);
        let m = l.margin.as_ref().unwrap();
        assert_eq!(
            (m.profit_each_cents, m.lot_profit_cents, m.margin_tenths),
            (1_800, 18_000, 254)
        );
        assert_eq!(l.margin_tenths_if_switched, Some(254));
        assert_eq!(l.cost_as_unit_cents, Some(5_000));
        assert_eq!(l.cost_as_lot_cents, 50_000);
        let s = &reply.summary;
        assert_eq!(
            (
                s.goods_cents,
                s.extra_billed_cents,
                s.total_cents,
                s.paid_to_others_cents
            ),
            (50_000, 0, 50_000, 3_000)
        );
        assert_eq!(
            (
                s.paying_now_cents,
                s.left_owed_cents,
                s.cash_out_today_cents
            ),
            (50_000, 0, 53_000)
        );
        assert!(!reply.charges_incomplete);
        assert!(reply.involves_cash);
        assert!(reply.blockers.is_empty(), "{:?}", reply.blockers);
    }

    #[test]
    fn a_supplier_charge_is_spread_by_value_to_the_cent() {
        let req = request(
            vec![line(1, 100), line(1, 300)],
            vec![charge(
                ChargeKind::Insurance,
                40,
                BilledBy::Supplier,
                Some(Allocation::Value),
            )],
        );
        let reply = compute(&req).unwrap();
        assert_eq!(reply.lines[0].landed_cents, 110);
        assert_eq!(reply.lines[1].landed_cents, 330);
        assert_eq!(reply.summary.extra_billed_cents, 40);
        assert_eq!(reply.summary.total_cents, 440);
    }

    #[test]
    fn a_half_filled_charge_is_not_counted_and_says_so() {
        let mut half = charge(ChargeKind::Shipping, 500, BilledBy::Supplier, None);
        half.billed_by = None;
        let reply = compute(&request(vec![line(2, 1_000)], vec![half])).unwrap();
        assert!(reply.charges_incomplete);
        assert_eq!(reply.lines[0].landed_cents, 2_000);
        assert!(
            reply
                .blockers
                .iter()
                .any(|b| b.code == BlockerCode::ChargeBilledByMissing)
        );
    }

    #[test]
    fn a_value_charge_over_unpriced_lines_waits_instead_of_dividing_by_zero() {
        let req = request(
            vec![line(1, 0), line(1, 0)],
            vec![charge(
                ChargeKind::Shipping,
                500,
                BilledBy::Supplier,
                Some(Allocation::Value),
            )],
        );
        let reply = compute(&req).unwrap();
        assert!(reply.charges_incomplete);
    }

    #[test]
    fn a_lot_that_does_not_divide_has_no_price_each() {
        let mut lot = line(3, 0);
        lot.cost_mode = CostMode::Lot;
        lot.cost_cents = 1_000;
        let reply = compute(&request(vec![lot], vec![])).unwrap();
        assert_eq!(reply.lines[0].cost_as_unit_cents, None);
        assert_eq!(reply.lines[0].unit_cost_cents, 333);
    }

    #[test]
    fn blockers_come_in_the_order_the_form_says_them() {
        let mut req = request(vec![line(0, 0)], vec![]);
        req.supplier_key = None;
        req.date = "2026-02-30".into();
        let codes: Vec<_> = compute(&req)
            .unwrap()
            .blockers
            .iter()
            .map(|b| b.code)
            .collect();
        assert_eq!(
            codes[..3],
            [
                BlockerCode::SupplierMissing,
                BlockerCode::DateInvalid,
                BlockerCode::QuantityMissing
            ]
        );
    }

    #[test]
    fn discounts_bigger_than_the_line_block_saving() {
        let req = request(
            vec![line(1, 100)],
            vec![charge(ChargeKind::Discount, 150, BilledBy::Supplier, None)],
        );
        let codes: Vec<_> = compute(&req)
            .unwrap()
            .blockers
            .iter()
            .map(|b| b.code)
            .collect();
        assert!(codes.contains(&BlockerCode::DiscountsExceedLine));
        assert!(codes.contains(&BlockerCode::DiscountsExceedBill));
    }

    #[test]
    fn out_of_range_input_is_refused() {
        let mut req = request(vec![line(1, 100)], vec![]);
        req.lines[0].cost_cents = -1;
        assert!(compute(&req).is_err());
        req.lines[0].cost_cents = MAX_CENTS;
        req.lines[0].quantity = Some(MAX_QUANTITY);
        assert!(compute(&req).is_err());
    }
}
