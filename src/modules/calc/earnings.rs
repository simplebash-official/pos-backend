// What a repair or print job earns and what the assigned employee's commission comes to, for the
// job forms and the employee screen's sample. Pure; the maths is `pos_calc_core`.

use crate::core::error::AppResult;

pub use pos_calc_core::earnings::{CalcEarningsReply, CalcEarningsRequest, SplitKind};

pub fn compute(request: &CalcEarningsRequest) -> AppResult<CalcEarningsReply> {
    Ok(pos_calc_core::earnings::compute(request)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        price: Option<i64>,
        material: i64,
        kind: Option<SplitKind>,
        value: f64,
    ) -> CalcEarningsRequest {
        CalcEarningsRequest {
            price_cents: price,
            material_cost_cents: material,
            split_type: kind,
            split_value: value,
        }
    }

    #[test]
    fn a_percentage_is_taken_of_the_profit() {
        // Rs 10,000 job, Rs 4,000 of parts: Rs 6,000 profit, 30% is Rs 1,800.
        let reply = compute(&request(
            Some(1_000_000),
            400_000,
            Some(SplitKind::Percentage),
            30.0,
        ))
        .unwrap();
        assert_eq!(
            (reply.profit_cents, reply.commission_cents),
            (600_000, 180_000)
        );
        assert_eq!(reply.shop_keeps_cents, 420_000);
        assert!(!reply.capped);
    }

    #[test]
    fn a_fixed_commission_is_cut_down_to_the_profit() {
        let reply = compute(&request(
            Some(100_000),
            70_000,
            Some(SplitKind::Fixed),
            50_000.0,
        ))
        .unwrap();
        assert_eq!(
            (reply.profit_cents, reply.commission_cents),
            (30_000, 30_000)
        );
        assert!(reply.capped);
        assert_eq!(reply.shop_keeps_cents, 0);
        let enough = compute(&request(
            Some(100_000),
            70_000,
            Some(SplitKind::Fixed),
            20_000.0,
        ))
        .unwrap();
        assert_eq!(enough.commission_cents, 20_000);
        assert!(!enough.capped);
    }

    #[test]
    fn a_loss_earns_no_commission_and_no_split_means_none() {
        let loss = compute(&request(
            Some(50_000),
            80_000,
            Some(SplitKind::Fixed),
            10_000.0,
        ))
        .unwrap();
        assert_eq!(
            (loss.profit_cents, loss.commission_cents, loss.capped),
            (0, 0, false)
        );
        let none = compute(&request(Some(50_000), 0, None, 0.0)).unwrap();
        assert_eq!((none.profit_cents, none.commission_cents), (50_000, 0));
    }

    #[test]
    fn nothing_is_worked_out_until_a_price_is_typed() {
        let reply = compute(&request(None, 1_000, Some(SplitKind::Percentage), 30.0)).unwrap();
        assert!(!reply.price_entered);
        assert_eq!(reply.commission_cents, 0);
    }

    #[test]
    fn half_cents_round_up_and_bad_values_are_refused() {
        // 12.5% of Rs 0.10 is 1.25 cents -> 1.
        let reply = compute(&request(Some(10), 0, Some(SplitKind::Percentage), 12.5)).unwrap();
        assert_eq!(reply.commission_cents, 1);
        assert!(compute(&request(Some(10), 0, Some(SplitKind::Percentage), 101.0)).is_err());
        assert!(compute(&request(Some(-1), 0, None, 0.0)).is_err());
        assert!(compute(&request(Some(10), 0, Some(SplitKind::Fixed), f64::NAN)).is_err());
    }
}
