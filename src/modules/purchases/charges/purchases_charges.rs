use crate::domain::purchase_receipts::{Allocation, BilledBy, ChargeKind};

/// Spreads `amount_cents` (signed) over lines in proportion to `weights`, to the cent: each line
/// first gets its whole-cent share, then the cents left over go one each to the lines with the
/// largest fractional remainders (ties to the earlier line). The shares always add up to the
/// amount. The weights must be positive and not empty.
#[must_use]
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
    shares
        .into_iter()
        .map(|share| (share * sign) as i64)
        .collect()
}

/// One stock batch of a line: how many units and what each costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchCost {
    pub quantity: i64,
    pub unit_cost_cents: i64,
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

#[must_use]
/// Totals the charges of a receipt by kind.
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
#[must_use]
pub fn owed_total(goods_cents: i64, totals: ChargeTotals) -> i64 {
    goods_cents + totals.supplier_billed_cents
}

/// The landed total per unit to the nearest cent, for display. Stock is costed with `split_cost`.
#[must_use]
pub fn rounded_unit(total_cents: i64, quantity: i64) -> i64 {
    if quantity <= 0 {
        return 0;
    }
    (total_cents + quantity / 2).div_euclid(quantity)
}

/// The amount of a charge with its sign: a discount takes money off, everything else adds.
#[must_use]
pub fn signed_amount(kind: ChargeKind, amount_cents: i64) -> i64 {
    if kind == ChargeKind::Discount {
        -amount_cents
    } else {
        amount_cents
    }
}

/// Splits a line's landed total over its units so the batches add up to it exactly. If the total
/// divides evenly it is one batch; otherwise `remainder` units cost one cent more than the rest
/// (two batches, the cheaper one first). `quantity` must be at least 1 and `total_cents` positive.
#[must_use]
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

/// The weight each line carries when a charge is spread: what it costs, or how many units it has.
#[must_use]
pub fn weights(allocation: Allocation, lines: &[(i64, i64)]) -> Vec<i64> {
    lines
        .iter()
        .map(|&(quantity, line_total_cents)| match allocation {
            Allocation::Value => line_total_cents,
            Allocation::Quantity => quantity,
        })
        .collect()
}
