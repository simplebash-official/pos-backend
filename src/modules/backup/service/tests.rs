use serde_json::{Value, json};
use std::collections::HashMap;

use super::*;

#[test]
fn a_pre_batch_backup_gets_opening_batches_before_restore() {
    let mut tables: HashMap<String, Vec<Value>> = HashMap::new();
    tables.insert(
        "products".into(),
        vec![
            json!({ "key": "prod_a", "id": "aaa", "cost_price_cents": 100_000,
                         "selling_price_cents": 120_000, "stock_quantity": 5,
                         "created_at": "2026-01-01T00:00:00Z", "deleted_at": null }),
        ],
    );
    tables.insert(
        "stock_movements".into(),
        vec![json!({ "key": "sm_1", "product_id": "aaa", "quantity_delta": 5 })],
    );
    tables.insert(
        "product_serials".into(),
        vec![json!({ "key": "psn_1", "product_key": "prod_a" })],
    );
    tables.insert(
        "purchases".into(),
        vec![json!({ "key": "pur_1", "product_key": "prod_a" })],
    );
    tables.insert(
        "invoices".into(),
        vec![json!({ "key": "inv_1",
                         "items": "[{\"productKey\":\"prod_a\",\"quantity\":2}]" })],
    );

    upgrade_pre_batch_backup(&mut tables);

    let batch = &tables["stock_batches"][0];
    assert_eq!(batch["key"], "stb_open_prod_a");
    assert_eq!(batch["remaining_quantity"], 5);
    assert_eq!(batch["selling_price_cents"], 120_000);
    assert_eq!(tables["stock_movements"][0]["batch_key"], "stb_open_prod_a");
    assert_eq!(tables["product_serials"][0]["batch_key"], "stb_open_prod_a");
    assert_eq!(tables["purchases"][0]["selling_price_cents"], 120_000);
    let items: Value =
        serde_json::from_str(tables["invoices"][0]["items"].as_str().unwrap()).unwrap();
    assert_eq!(items[0]["batchAllocations"][0]["unitCostCents"], 100_000);
}

#[test]
fn purchases_of_a_pre_receipt_backup_are_grouped_into_receipts() {
    let mut tables: HashMap<String, Vec<Value>> = HashMap::new();
    tables.insert(
            "purchases".into(),
            vec![
                json!({ "key": "pur_b", "supplier_key": "sup_1", "reference_no": "INV-1", "date": "2026-01-01T00:00:00Z", "created_at": "2026-01-01T00:00:00Z" }),
                json!({ "key": "pur_a", "supplier_key": "sup_1", "reference_no": " INV-1 ", "date": "2026-01-01T00:00:00Z", "created_at": "2026-01-01T00:00:00Z" }),
                json!({ "key": "pur_c", "supplier_key": "sup_1", "date": "2026-01-01T00:00:00Z", "created_at": "2026-01-01T00:00:00Z" }),
            ],
        );

    upgrade_pre_receipt_backup(&mut tables);

    let purchases = &tables["purchases"];
    assert_eq!(purchases[0]["receipt_key"], "prc_leg_pur_a");
    assert_eq!(purchases[1]["receipt_key"], "prc_leg_pur_a");
    assert_eq!(purchases[2]["receipt_key"], "prc_leg_pur_c");
    assert!(purchases[0].get("reference_no").is_none());
    let receipts = &tables["purchase_receipts"];
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0]["reference_no"], "INV-1");
    assert_eq!(receipts[0]["receipt_number"], "PUR-Lpur_a");
}

#[test]
fn purchase_lines_priced_per_unit_get_a_line_total() {
    let mut tables: HashMap<String, Vec<Value>> = HashMap::new();
    tables.insert(
            "purchases".into(),
            vec![
                json!({ "key": "pur_a", "quantity": 4, "unit_cost_cents": 250 }),
                json!({ "key": "pur_b", "quantity": 2, "line_total_cents": 900, "charges_share_cents": 10 }),
            ],
        );
    upgrade_pre_line_total_backup(&mut tables);
    let purchases = &tables["purchases"];
    assert_eq!(purchases[0]["line_total_cents"], 1000);
    assert_eq!(purchases[0]["charges_share_cents"], 0);
    assert!(purchases[0].get("unit_cost_cents").is_none());
    assert_eq!(purchases[1]["line_total_cents"], 900);
    assert_eq!(purchases[1]["charges_share_cents"], 10);
}

#[test]
fn a_backup_with_batches_is_left_alone() {
    let mut tables: HashMap<String, Vec<Value>> = HashMap::new();
    tables.insert(
        "products".into(),
        vec![json!({ "key": "prod_a", "id": "aaa", "stock_quantity": 5 })],
    );
    let before = tables.clone();
    upgrade_pre_batch_backup(&mut tables);
    assert_eq!(tables, before);
}
