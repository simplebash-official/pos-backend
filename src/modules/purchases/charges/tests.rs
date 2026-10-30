use super::purchases_charges::BatchCost;
use super::purchases_charges::ChargeFigure;
use super::purchases_charges::allocate;
use super::purchases_charges::charge_totals;
use super::purchases_charges::owed_total;
use super::purchases_charges::rounded_unit;
use super::purchases_charges::split_cost;
use super::purchases_charges::weights;
use crate::domain::purchase_receipts::{Allocation, BilledBy, ChargeKind};

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
            assert_eq!(
                shares.iter().sum::<i64>(),
                amount,
                "{amount} over {weights:?}"
            );
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
                batches
                    .iter()
                    .map(|b| b.quantity * b.unit_cost_cents)
                    .sum::<i64>(),
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
