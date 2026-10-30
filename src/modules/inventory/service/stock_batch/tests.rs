use super::allocations_for_return::allocations_for_return;
use super::backfill_line_allocations::backfill_line_allocations;
use super::plan_from_batches::plan_from_batches;
use super::tiers_from_batches::tiers_from_batches;
use super::weighted_unit_cost::weighted_unit_cost;

use crate::domain::inventory::{BatchAllocation, BatchSource};
use crate::modules::inventory::model::StockBatchDocument;

use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;

fn batch(key: &str, price: i64, cost: i64, remaining: i64, day: i64) -> StockBatchDocument {
    StockBatchDocument {
        id: Some(ObjectId::new()),
        key: key.to_string(),
        product_key: "prod_a".to_string(),
        source: BatchSource::Purchase,
        purchase_key: None,
        supplier_key: None,
        received_quantity: remaining,
        remaining_quantity: remaining,
        unit_cost_cents: cost,
        selling_price_cents: price,
        received_at: BsonDateTime::from_millis(day * 86_400_000),
        version: 1,
        created_at: BsonDateTime::from_millis(0),
        updated_at: BsonDateTime::from_millis(0),
        deleted_at: None,
        updated_by_device: None,
    }
}

// Item A: 5 left of the Rs.1000/1200 delivery, 12 of the Rs.1100/1500 one.
fn item_a() -> Vec<StockBatchDocument> {
    vec![
        batch("stb_1", 120_000, 100_000, 5, 1),
        batch("stb_2", 150_000, 110_000, 12, 2),
    ]
}

#[test]
fn each_selling_price_is_its_own_tier() {
    let tiers = tiers_from_batches(&item_a());
    assert_eq!(tiers.len(), 2);
    assert_eq!(
        (tiers[0].selling_price_cents, tiers[0].quantity),
        (120_000, 5)
    );
    assert_eq!(
        (tiers[1].selling_price_cents, tiers[1].quantity),
        (150_000, 12)
    );
    assert_eq!(tiers[1].min_unit_cost_cents, Some(110_000));
}

#[test]
fn same_price_batches_merge_and_empty_ones_drop_out() {
    let batches = vec![
        batch("stb_1", 150_000, 100_000, 3, 1),
        batch("stb_2", 150_000, 110_000, 4, 2),
        batch("stb_3", 120_000, 90_000, 0, 3),
    ];
    let tiers = tiers_from_batches(&batches);
    assert_eq!(tiers.len(), 1);
    assert_eq!(tiers[0].quantity, 7);
    assert_eq!(tiers[0].min_unit_cost_cents, Some(100_000));
    assert_eq!(tiers[0].max_unit_cost_cents, Some(110_000));
    assert_eq!(
        tiers[0].oldest_received_at,
        BsonDateTime::from_millis(86_400_000).to_chrono()
    );
}

#[test]
fn a_sale_draws_only_on_its_tier() {
    let plan = plan_from_batches(&item_a(), "Item A", 150_000, 3).unwrap();
    assert_eq!(
        plan,
        vec![BatchAllocation {
            batch_key: "stb_2".into(),
            quantity: 3,
            unit_cost_cents: 110_000
        }]
    );
}

#[test]
fn a_tier_is_consumed_oldest_first_across_batches() {
    let batches = vec![
        batch("stb_old", 150_000, 100_000, 2, 1),
        batch("stb_new", 150_000, 110_000, 10, 2),
    ];
    let plan = plan_from_batches(&batches, "Item A", 150_000, 5).unwrap();
    assert_eq!(plan.len(), 2);
    assert_eq!(
        (plan[0].batch_key.as_str(), plan[0].quantity),
        ("stb_old", 2)
    );
    assert_eq!(
        (plan[1].batch_key.as_str(), plan[1].quantity),
        ("stb_new", 3)
    );
    assert_eq!(weighted_unit_cost(&plan), (2 * 100_000 + 3 * 110_000) / 5);
}

#[test]
fn asking_for_more_than_the_tier_holds_is_refused() {
    let err = plan_from_batches(&item_a(), "Item A", 120_000, 6).unwrap_err();
    assert!(format!("{err:?}").contains("INSUFFICIENT_STOCK"));
}

#[test]
fn returns_go_back_newest_batch_first_and_continue_where_the_last_stopped() {
    let line = vec![
        BatchAllocation {
            batch_key: "stb_old".into(),
            quantity: 2,
            unit_cost_cents: 100,
        },
        BatchAllocation {
            batch_key: "stb_new".into(),
            quantity: 3,
            unit_cost_cents: 110,
        },
    ];
    let first = allocations_for_return(&line, 0, 2);
    assert_eq!(
        first,
        vec![BatchAllocation {
            batch_key: "stb_new".into(),
            quantity: 2,
            unit_cost_cents: 110
        }]
    );
    let second = allocations_for_return(&line, 2, 3);
    assert_eq!(
        second,
        vec![
            BatchAllocation {
                batch_key: "stb_new".into(),
                quantity: 1,
                unit_cost_cents: 110
            },
            BatchAllocation {
                batch_key: "stb_old".into(),
                quantity: 2,
                unit_cost_cents: 100
            },
        ]
    );
}

#[test]
fn migration_gives_old_product_lines_their_opening_batch() {
    let mut items = serde_json::json!([
        { "productKey": "prod_a", "quantity": 2, "unitCostCents": 900 },
        { "productKey": "prod_b", "quantity": 1 },
        { "name": "Screen repair", "quantity": 1, "sourceType": "repair" },
        { "productKey": "prod_c", "quantity": 1,
          "batchAllocations": [{ "batchKey": "stb_x", "quantity": 1, "unitCostCents": 5 }] },
    ]);
    let changed = backfill_line_allocations(&mut items, |key| (key == "prod_b").then_some(700));
    assert!(changed);
    assert_eq!(
        items[0]["batchAllocations"][0]["batchKey"],
        "stb_open_prod_a"
    );
    assert_eq!(items[0]["batchAllocations"][0]["unitCostCents"], 900);
    assert_eq!(items[1]["batchAllocations"][0]["unitCostCents"], 700);
    assert!(items[2].get("batchAllocations").is_none());
    assert_eq!(items[3]["batchAllocations"][0]["batchKey"], "stb_x");
    assert!(!backfill_line_allocations(&mut items, |_| None));
}

#[test]
fn a_price_no_batch_sells_at_is_refused() {
    let err = plan_from_batches(&item_a(), "Item A", 130_000, 1).unwrap_err();
    assert!(format!("{err:?}").contains("PRICE_TIER_UNAVAILABLE"));
}
