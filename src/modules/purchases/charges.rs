// The money maths of a receipt's extra charges, with no I/O: how a charge is spread over the
// receipt's lines, how a line's landed cost is split into stock batches, and what a receipt owes
// the supplier. Everything is integer cents and exact: shares add up to the charge to the cent,
// and the batches of a line add up to its landed total to the cent. The frontend has a twin of
// the spreading and splitting (`features/purchases/lib/landedCost.ts`) that is tested against the
// same vectors, so the form previews exactly what the server will save.

use crate::domain::purchase_receipts::{Allocation, BilledBy, ChargeKind};

/// The amount of a charge with its sign: a discount takes money off, everything else adds.
pub fn signed_amount(kind: ChargeKind, amount_cents: i64) -> i64 {
    if kind == ChargeKind::Discount {
        -amount_cents
    } else {
        amount_cents
    }
}

/// The weight each line carries when a charge is spread: what it costs, or how many units it has.
pub fn weights(allocation: Allocation, lines: &[(i64, i64)]) -> Vec<i64> {
    lines
        .iter()
        .map(|&(quantity, line_total_cents)| match allocation {
            Allocation::Value => line_total_cents,
            Allocation::Quantity => quantity,
        })
        .collect()
}

/// Spreads `amount_cents` (signed) over lines in proportion to `weights`, to the cent: each line
/// first gets its whole-cent share, then the cents left over go one each to the lines with the
/// largest fractional remainders (ties to the earlier line). The shares always add up to the
/// amount. The weights must be positive and not empty.
pub fn allocate(amount_cents: i64, weights: &[i64]) -> Vec<i64> {
    let total_weight: i128 = weights.iter().map(|&w| i128::from(w)).sum();
    debug_assert!(total_weight > 0, "weights must add up to something");
    let magnitude = i128::from(amount_cents.unsigned_abs());

    let mut shares: Vec<i128> = Vec::with_capacity(weights.len());
    let mut remainders: Vec<(i128, usize)> = Vec::with_capacity(weights.len());
    for (index, &weight) in weights.iter().enumerate() {
        let scaled = magnitude * i128::from(weight);
        shares.push(scaled / total_weight);
        remainders.push((scaled % total_weight, index));
    }
    let leftover = magnitude - shares.iter().sum::<i128>();
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for &(_, index) in remainders.iter().take(leftover as usize) {
        shares[index] += 1;
    }

    let sign: i128 = if amount_cents < 0 { -1 } else { 1 };
    shares.into_iter().map(|share| (share * sign) as i64).collect()
}

/// One stock batch of a line: how many units and what each costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchCost {
    pub quantity: i64,
    pub unit_cost_cents: i64,
}

/// Splits a line's landed total over its units so the batches add up to it exactly. If the total
/// divides evenly it is one batch; otherwise `remainder` units cost one cent more than the rest
/// (two batches, the cheaper one first). `quantity` must be at least 1 and `total_cents` positive.
pub fn split_cost(total_cents: i64, quantity: i64) -> Vec<BatchCost> {
    let each = total_cents / quantity;
    let remainder = total_cents % quantity;
    if remainder == 0 {
        vec![BatchCost {
            quantity,
            unit_cost_cents: each,
        }]
    } else {
        vec![
            BatchCost {
                quantity: quantity - remainder,
                unit_cost_cents: each,
            },
            BatchCost {
                quantity: remainder,
                unit_cost_cents: each + 1,
            },
        ]
    }
}

/// The landed total per unit to the nearest cent, for display. Stock is costed with `split_cost`.
pub fn rounded_unit(total_cents: i64, quantity: i64) -> i64 {
    if quantity <= 0 {
        return 0;
    }
    (total_cents + quantity / 2).div_euclid(quantity)
}

/// One extra charge reduced to what the owed-total needs.
#[derive(Debug, Clone, Copy)]
pub struct ChargeFigure {
    pub kind: ChargeKind,
    pub amount_cents: i64,
    pub billed_by: BilledBy,
}

/// What the receipt's charges come to, split by who is paid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChargeTotals {
    /// On the supplier's bill, discounts taken off. Changes what is owed to the supplier.
    pub supplier_billed_cents: i64,
    /// Paid to someone else. Never owed to the supplier.
    pub paid_to_others_cents: i64,
}

pub fn charge_totals(charges: &[ChargeFigure]) -> ChargeTotals {
    let mut totals = ChargeTotals::default();
    for charge in charges {
        let amount = signed_amount(charge.kind, charge.amount_cents);
        match charge.billed_by {
            BilledBy::Supplier => totals.supplier_billed_cents += amount,
            BilledBy::ThirdParty => totals.paid_to_others_cents += amount,
        }
    }
    totals
}

/// What a receipt owes the supplier before credits and payments: the goods plus the charges the
/// supplier billed. The one definition behind the receipt total and the supplier balance.
pub fn owed_total(goods_cents: i64, totals: ChargeTotals) -> i64 {
    goods_cents + totals.supplier_billed_cents
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_adds_up_to_the_cent() {
        // 1000 over three equal lines: 334, 333, 333 (the extra cent goes to the earliest).
        assert_eq!(allocate(1000, &[1, 1, 1]), vec![334, 333, 333]);
        // 100 over 1:2:3 = 16.67, 33.33, 50 -> 17, 33, 50.
        assert_eq!(allocate(100, &[1, 2, 3]), vec![17, 33, 50]);
        // The largest remainder wins the cent, not the first line.
        assert_eq!(allocate(10, &[1, 1, 8]), vec![1, 1, 8]);
        assert_eq!(allocate(7, &[1, 2]), vec![2, 5]);
        // A single line takes everything.
        assert_eq!(allocate(12_345, &[40]), vec![12_345]);
    }

    #[test]
    fn a_discount_is_spread_the_same_way_and_negated() {
        assert_eq!(allocate(-1000, &[1, 1, 1]), vec![-334, -333, -333]);
        assert_eq!(allocate(-100, &[1, 2, 3]), vec![-17, -33, -50]);
        assert_eq!(allocate(-100, &[1, 2, 3]).iter().sum::<i64>(), -100);
    }

    #[test]
    fn allocation_always_sums_to_the_amount() {
        for amount in [1, 2, 3, 99, 1000, 12_345, 1_000_001] {
            for weights in [
                vec![1, 1],
                vec![3, 7, 11],
                vec![85_000, 12_500, 999],
                vec![1, 1, 1, 1, 1, 1, 1],
            ] {
                let shares = allocate(amount, &weights);
                assert_eq!(shares.iter().sum::<i64>(), amount, "{amount} over {weights:?}");
                assert!(shares.iter().all(|&s| s >= 0));
            }
        }
    }

    #[test]
    fn weights_follow_the_method() {
        let lines = [(10, 5_000), (30, 1_000)];
        assert_eq!(weights(Allocation::Value, &lines), vec![5_000, 1_000]);
        assert_eq!(weights(Allocation::Quantity, &lines), vec![10, 30]);
    }

    #[test]
    fn an_even_lot_is_one_batch() {
        assert_eq!(
            split_cost(85_000, 100),
            vec![BatchCost {
                quantity: 100,
                unit_cost_cents: 850
            }]
        );
    }

    #[test]
    fn an_uneven_lot_is_two_batches_one_cent_apart_that_add_up_exactly() {
        // Rs 50,000.00 for 30 pieces = 5,000,000 cents / 30 = 166,666 r 20.
        let batches = split_cost(5_000_000, 30);
        assert_eq!(
            batches,
            vec![
                BatchCost {
                    quantity: 10,
                    unit_cost_cents: 166_666
                },
                BatchCost {
                    quantity: 20,
                    unit_cost_cents: 166_667
                },
            ]
        );
        let cost: i64 = batches.iter().map(|b| b.quantity * b.unit_cost_cents).sum();
        let units: i64 = batches.iter().map(|b| b.quantity).sum();
        assert_eq!((cost, units), (5_000_000, 30));
    }

    #[test]
    fn splitting_is_exact_for_many_totals() {
        for quantity in 1..=40 {
            for total in [1, 7, 99, 1000, 9_999, 123_457] {
                let batches = split_cost(total, quantity);
                assert!(batches.len() <= 2);
                assert_eq!(batches.iter().map(|b| b.quantity).sum::<i64>(), quantity);
                assert_eq!(
                    batches.iter().map(|b| b.quantity * b.unit_cost_cents).sum::<i64>(),
                    total
                );
            }
        }
    }

    #[test]
    fn the_unit_shown_is_the_nearest_cent() {
        assert_eq!(rounded_unit(5_000_000, 30), 166_667);
        assert_eq!(rounded_unit(85_000, 100), 850);
        assert_eq!(rounded_unit(10, 4), 3);
    }

    #[test]
    fn only_the_suppliers_charges_change_what_is_owed() {
        let totals = charge_totals(&[
            ChargeFigure {
                kind: ChargeKind::Shipping,
                amount_cents: 2_000,
                billed_by: BilledBy::Supplier,
            },
            ChargeFigure {
                kind: ChargeKind::Discount,
                amount_cents: 500,
                billed_by: BilledBy::Supplier,
            },
            ChargeFigure {
                kind: ChargeKind::Shipping,
                amount_cents: 700,
                billed_by: BilledBy::ThirdParty,
            },
        ]);
        assert_eq!(totals.supplier_billed_cents, 1_500);
        assert_eq!(totals.paid_to_others_cents, 700);
        assert_eq!(owed_total(100_000, totals), 101_500);
    }
}
