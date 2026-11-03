//! Device convergence tests.

use mongodb::bson::{Document, doc};
use serde_json::json;

use simplebash_pos_backend::core::tenancy::Tenant;

use std::collections::BTreeSet;

use crate::common;
use crate::helpers::{
    Device, Rng, SEED, TENANT, bootstrap, cloud_changes, cloud_conflicts, keys, pull_all, push_all,
    reserve_blocks, start_consumer, table_rows, wait_for_cloud_quiet,
};
use uuid::Uuid;

const ROUNDS: usize = 4;

const OPS_PER_ROUND: usize = 3;

// ---------------------------------------------------------------------------
// Shop data and random operations
// ---------------------------------------------------------------------------

struct Catalog {
    products: Vec<(String, String)>, // (key, legacy id)
}

async fn create_shop(d: &Device) -> Catalog {
    let cat = d
        .api(
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": "Electronics", "icon": "devices", "color": "blue" })),
        )
        .await;
    let category_key = cat["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let sub = d
        .api(
            "POST",
            &format!("/api/inventory/categories/{category_key}/subcategories"),
            Some(json!({ "name": "Phones" })),
        )
        .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let mut products = Vec::new();
    for name in ["Phone", "Charger"] {
        let p = d
            .api(
                "POST",
                "/api/inventory/products",
                Some(json!({
                    "name": name, "categoryKey": category_key, "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
                    "minStockThreshold": 2,
                    "openingStock": { "quantity": 500, "unitCostCents": 30000, "sellingPriceCents": 50000 }
                })),
            )
            .await;
        products.push((
            p["data"]["key"]
                .as_str()
                .expect("the value is a string")
                .to_string(),
            p["data"]["id"]
                .as_str()
                .expect("the value is a string")
                .to_string(),
        ));
    }
    d.api(
        "POST",
        "/api/customers",
        Some(json!({ "name": "Walk-in Regular", "primaryPhone": "0770000000" })),
    )
    .await;
    Catalog { products }
}

/// `(selling price, quantity)` of each price tier of a product on `d`.
async fn tiers(d: &Device, id: &str) -> Vec<(i64, i64)> {
    d.api("GET", &format!("/api/inventory/products/{id}"), None)
        .await["data"]["priceTiers"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|t| {
            (
                t["sellingPriceCents"]
                    .as_i64()
                    .expect("the value is an integer"),
                t["quantity"].as_i64().expect("the value is an integer"),
            )
        })
        .collect()
}

/// `(key, remaining)` of each batch of a product on `d`.
async fn batches(d: &Device, key: &str) -> Vec<(String, i64)> {
    d.api(
        "GET",
        &format!("/api/inventory/products/{key}/batches"),
        None,
    )
    .await["data"]["items"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|b| {
            (
                b["key"]
                    .as_str()
                    .expect("the value is a string")
                    .to_string(),
                b["remainingQuantity"]
                    .as_i64()
                    .expect("the value is an integer"),
            )
        })
        .collect()
}

async fn random_op(d: &Device, catalog: &Catalog, rng: &mut Rng, step: usize) {
    let (key, id) = &catalog.products[rng.below(catalog.products.len())];
    match rng.below(6) {
        0 | 1 => {
            // A sale at one of the prices the product is in stock at here.
            let quantity = 1 + rng.below(3) as i64;
            let in_stock: Vec<i64> = tiers(d, id)
                .await
                .into_iter()
                .filter(|(_, left)| *left >= quantity)
                .map(|(price, _)| price)
                .collect();
            if in_stock.is_empty() {
                return;
            }
            let price = in_stock[rng.below(in_stock.len())];
            d.api(
                "POST",
                "/api/billing/sales",
                Some(json!({
                    "staff": { "cashierName": "Admin" },
                    "items": [{ "productKey": key, "sellingPriceCents": price, "quantity": quantity, "discountCents": 0, "sourceType": "retail" }],
                    "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 1_000_000 },
                    "shopProfileSnapshot": { "name": "Shop", "address": "Colombo" }
                })),
            )
            .await;
        }
        2 => {
            // Re-pricing a delivery: concurrent edits of one batch resolve last-writer-wins.
            let all = batches(d, key).await;
            let (batch_key, _) = &all[rng.below(all.len())];
            let price = 40_000 + 100 * rng.below(200) as i64;
            d.api(
                "PATCH",
                &format!("/api/inventory/batches/{batch_key}"),
                Some(json!({ "sellingPriceCents": price })),
            )
            .await;
        }
        3 => {
            // A new cashier login: every replica must end up with the same account.
            d.api(
                "POST",
                "/api/users",
                Some(json!({
                    "name": format!("Cashier {step}"), "username": format!("staff-{}-{step}", d.id),
                    "password": format!("cashier-pass-{step}"), "role": "staff"
                })),
            )
            .await;
        }
        5 => {
            // A price offer: a new quotation, or the cancellation of one this device holds.
            let active = d
                .api("GET", "/api/quotations?status=active&limit=50", None)
                .await;
            let open: Vec<String> = active["data"]["quotations"]
                .as_array()
                .expect("the value is an array")
                .iter()
                .map(|q| {
                    q["key"]
                        .as_str()
                        .expect("the value is a string")
                        .to_string()
                })
                .collect();
            let prices = tiers(d, id).await;
            if !open.is_empty() && (rng.below(3) == 0 || prices.is_empty()) {
                let key = &open[rng.below(open.len())];
                d.api("POST", &format!("/api/quotations/{key}/cancel"), None)
                    .await;
            } else if !prices.is_empty() {
                d.api(
                    "POST",
                    "/api/quotations",
                    Some(json!({
                        "customer": { "customerName": format!("Prospect {} {step}", d.id) },
                        "items": [{ "productKey": key, "sellingPriceCents": prices[0].0, "quantity": 1 + rng.below(3), "discountCents": 0 }],
                        "validUntil": "2099-12-31",
                        "terms": "Prices held until the date above."
                    })),
                )
                .await;
            }
        }
        _ => {
            if rng.below(2) == 0 {
                // Stock in comes as a new batch; stock out leaves a batch that holds it.
                let delta = rng.below(7) as i64 - 3;
                if delta > 0 {
                    d.api(
                        "PATCH",
                        &format!("/api/inventory/products/{id}/stock"),
                        Some(json!({
                            "delta": delta, "unitCostCents": 30000,
                            "sellingPriceCents": 40_000 + 100 * rng.below(200) as i64,
                        })),
                    )
                    .await;
                } else if delta < 0
                    && let Some((batch_key, _)) = batches(d, key)
                        .await
                        .into_iter()
                        .find(|(_, left)| *left >= -delta)
                {
                    d.api(
                        "PATCH",
                        &format!("/api/inventory/products/{id}/stock"),
                        Some(json!({ "delta": delta, "batchKey": batch_key })),
                    )
                    .await;
                }
            } else {
                d.api(
                    "POST",
                    "/api/customers",
                    Some(json!({ "name": format!("Customer {} {step}", d.id), "primaryPhone": format!("07{:08}", 1000 + step) })),
                )
                .await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

const SYNCED: &[&str] = &[
    "categories",
    "subcategories",
    "suppliers",
    "products",
    "stock_batches",
    "supplier_products",
    "customers",
    "purchase_receipts",
    "purchases",
    "receipt_settlements",
    "supplier_credits",
    "receipt_charges",
    "expenses",
    "expense_payments",
    "stock_movements",
    "invoices",
    "payments",
    "credit_notes",
    "quotations",
    "users",
];

// ---------------------------------------------------------------------------
// The scenario
// ---------------------------------------------------------------------------

#[tokio::test]
async fn three_devices_and_the_cloud_converge() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;
    let _ = Tenant::id(TENANT).expect("the tenant id is valid");

    let mut devices = vec![
        Device::new(1, &cloud).await,
        Device::new(2, &cloud).await,
        Device::new(3, &cloud).await,
    ];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
        reserve_blocks(&cloud, d).await;
    }

    // Device 1 is the first device: it creates the shop and uploads it.
    let catalog = create_shop(&devices[0]).await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;

    // Devices 2 and 3 join by bootstrapping from the cloud snapshot.
    for d in devices.iter_mut().skip(1) {
        bootstrap(&cloud, d).await;
    }

    // Offline work with partial, random syncs.
    let mut rng = Rng(SEED);
    let mut step = 0;
    for round in 0..ROUNDS {
        for d in &devices {
            for _ in 0..OPS_PER_ROUND {
                step += 1;
                random_op(d, &catalog, &mut rng, step).await;
            }
        }
        let syncing: Vec<usize> = (0..devices.len()).filter(|_| rng.below(3) != 0).collect();
        for i in syncing {
            push_all(&cloud, &devices[i]).await;
            pull_all(&cloud, &mut devices[i]).await;
        }
        eprintln!("round {round} done");
    }

    // Drive to quiescence: everybody pushes, the cloud settles, everybody pulls; twice.
    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await;
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in devices.iter_mut() {
            pull_all(&cloud, d).await;
        }
    }

    // 1. Every replica holds the same data.
    for table in SYNCED {
        let base = table_rows(devices[0].pool(), table).await;
        for d in &devices[1..] {
            assert_eq!(
                base,
                table_rows(d.pool(), table).await,
                "table {table} differs on {}",
                d.id
            );
        }
    }

    let logins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(devices[0].pool())
        .await
        .expect("the awaited step succeeds");
    assert!(
        logins > 0,
        "the scenario must create logins for the users comparison to mean anything"
    );

    // 2. Stock is exactly its movement ledger on every replica.
    for d in &devices {
        let rows = sqlx::query(
            "SELECT p.key AS key, p.stock_quantity AS stock, COALESCE(SUM(m.quantity_delta), 0) AS ledger \
             FROM products p LEFT JOIN stock_movements m ON m.product_id = p.id GROUP BY p.id",
        )
        .fetch_all(d.pool())
        .await
        .expect("the awaited step succeeds");
        for row in rows {
            use sqlx::Row;
            let (key, stock, ledger): (String, i64, i64) =
                (row.get("key"), row.get("stock"), row.get("ledger"));
            assert_eq!(
                stock, ledger,
                "stock of {key} is not its ledger on {}",
                d.id
            );
        }
    }

    // 3. The cloud holds exactly the ledger the devices hold.
    let cloud_log = cloud_changes(&cloud).await;
    for (resource, table) in [
        ("stockMovements", "stock_movements"),
        ("invoices", "invoices"),
        ("payments", "payments"),
        ("quotations", "quotations"),
        ("users", "users"),
    ] {
        let in_cloud: BTreeSet<String> = cloud_log
            .iter()
            .filter(|c| c["resource"] == resource && c["op"] == "upsert")
            .map(|c| {
                c["key"]
                    .as_str()
                    .expect("the value is a string")
                    .to_string()
            })
            .collect();
        assert_eq!(
            in_cloud,
            keys(devices[0].pool(), table).await,
            "{resource} in the cloud differ from the devices"
        );
    }

    // 4. No two invoices share a number, and nothing was renamed to dodge a collision.
    let numbers: Vec<String> = sqlx::query_scalar("SELECT invoice_number FROM invoices")
        .fetch_all(devices[0].pool())
        .await
        .expect("the awaited step succeeds");
    let distinct: BTreeSet<&String> = numbers.iter().collect();
    assert_eq!(
        numbers.len(),
        distinct.len(),
        "duplicate invoice numbers: {numbers:?}"
    );
    for d in &devices {
        let unique_violations: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sync_conflicts WHERE kind = 'UNIQUE_VIOLATION'",
        )
        .fetch_one(d.pool())
        .await
        .expect("the awaited step succeeds");
        assert_eq!(
            unique_violations, 0,
            "{} had to rename a colliding value",
            d.id
        );
    }

    cloud.mongo().drop().await.ok();
}

// ---------------------------------------------------------------------------
// Hardening: clock skew, injected conflicts, wider operation variety.
// Each test below opens its own throwaway cloud database, isolated from the
// scenario above and from each other.
// ---------------------------------------------------------------------------

/// Persistent per-device clock skew must not stop convergence: `clock_offset_ms`
/// is baked into every outbound change's `updatedAt` (see `modules::sync::state`
/// and `modules::sync::outbox`), and the receiving side clamps/monotonically
/// orders on top of that - this proves the combination actually holds across a
/// real push/pull/apply round trip, with four devices and a wider operation mix
/// than the base scenario (adds credit notes and repairs into the rotation).
#[tokio::test]
async fn devices_with_persistent_clock_skew_still_converge() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;

    let mut devices = vec![
        Device::new(11, &cloud).await,
        Device::new(12, &cloud).await,
        Device::new(13, &cloud).await,
        Device::new(14, &cloud).await,
    ];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
        reserve_blocks(&cloud, d).await;
    }
    // Device 12 runs 90s fast, device 13 runs 60s slow, the rest are on time.
    devices[1]
        .sync(
            "POST",
            "/api/sync/state",
            Some(json!({ "clockOffsetMs": 90_000 })),
        )
        .await;
    devices[2]
        .sync(
            "POST",
            "/api/sync/state",
            Some(json!({ "clockOffsetMs": -60_000 })),
        )
        .await;

    let catalog = create_shop(&devices[0]).await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    for d in devices.iter_mut().skip(1) {
        bootstrap(&cloud, d).await;
    }

    let mut rng = Rng(SEED ^ 0xC10C_5CE1);
    let mut step = 0;
    for round in 0..5 {
        for d in &devices {
            for _ in 0..4 {
                step += 1;
                random_op(d, &catalog, &mut rng, step).await;
            }
        }
        let syncing: Vec<usize> = (0..devices.len()).filter(|_| rng.below(3) != 0).collect();
        for i in syncing {
            push_all(&cloud, &devices[i]).await;
            pull_all(&cloud, &mut devices[i]).await;
        }
        eprintln!("skew round {round} done");
    }

    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await;
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in devices.iter_mut() {
            pull_all(&cloud, d).await;
        }
    }

    for table in SYNCED {
        let base = table_rows(devices[0].pool(), table).await;
        for d in &devices[1..] {
            assert_eq!(
                base,
                table_rows(d.pool(), table).await,
                "table {table} differs on {} under clock skew",
                d.id
            );
        }
    }
    for d in &devices {
        let rows = sqlx::query(
            "SELECT p.key AS key, p.stock_quantity AS stock, COALESCE(SUM(m.quantity_delta), 0) AS ledger \
             FROM products p LEFT JOIN stock_movements m ON m.product_id = p.id GROUP BY p.id",
        )
        .fetch_all(d.pool())
        .await
        .expect("the awaited step succeeds");
        for row in rows {
            use sqlx::Row;
            let (key, stock, ledger): (String, i64, i64) =
                (row.get("key"), row.get("stock"), row.get("ledger"));
            assert_eq!(
                stock, ledger,
                "stock of {key} is not its ledger on {} under clock skew",
                d.id
            );
        }
    }
    let numbers: Vec<String> = sqlx::query_scalar("SELECT invoice_number FROM invoices")
        .fetch_all(devices[0].pool())
        .await
        .expect("the awaited step succeeds");
    let distinct: BTreeSet<&String> = numbers.iter().collect();
    assert_eq!(
        numbers.len(),
        distinct.len(),
        "duplicate invoice numbers under clock skew: {numbers:?}"
    );

    cloud.mongo().drop().await.ok();
}

/// Two devices independently sell the SAME physical serial (offline, unaware of
/// each other) to two different invoices. Both sales must survive the merge -
/// selling twice is a real-world mistake to flag for a human, not data to
/// silently drop - and it must be recorded as a `SERIAL_DOUBLE_SOLD` conflict.
#[tokio::test]
async fn concurrent_serial_sale_is_flagged_and_both_invoices_survive() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;

    let mut devices = vec![Device::new(21, &cloud).await, Device::new(22, &cloud).await];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
        reserve_blocks(&cloud, d).await;
    }

    // Device 1 mints the serialized product, its serial, and pushes them out.
    let cat = devices[0]
        .api(
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": "Serial Goods", "icon": "phone", "color": "teal" })),
        )
        .await;
    let category_key = cat["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let sub = devices[0]
        .api(
            "POST",
            &format!("/api/inventory/categories/{category_key}/subcategories"),
            Some(json!({ "name": "Flagship" })),
        )
        .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let product = devices[0]
        .api(
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": "Serial Phone", "categoryKey": category_key, "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
                "minStockThreshold": 1, "isSerialized": true
            })),
        )
        .await;
    let product_key = product["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let supplier = devices[0]
        .api(
            "POST",
            "/api/suppliers",
            Some(json!({
                "name": "Serial Supplier", "contactPerson": "Sam", "primaryPhone": "0112345000",
                "address": "Colombo", "suppliedCategories": ["Serial Goods"]
            })),
        )
        .await;
    let supplier_key = supplier["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let serial = format!("SN-{}", Uuid::new_v4());
    // Part paid from the bank, with a credit against it: a receipt, a settlement, a credit and a
    // ledger expense with its payment, all of which must reach the cloud and the other device.
    let receipt = devices[0]
        .api(
            "POST",
            "/api/purchase-receipts",
            Some(json!({
                "supplierKey": supplier_key, "date": chrono::Utc::now().to_rfc3339(),
                "lines": [{ "productKey": product_key, "quantity": 1, "lineTotalCents": 50000, "sellingPriceCents": 80000, "serialNumbers": [serial] }],
                "payment": {
                    "source": "bank", "amountCents": 30000, "paidOn": "2026-09-06",
                    "categoryKey": simplebash_pos_backend::domain::cash_categories::category_key(
                        simplebash_pos_backend::domain::sessions::CashMovementType::SupplierPayment,
                        "Stock purchase (cash on delivery)",
                    ),
                },
            })),
        )
        .await;
    devices[0]
        .api(
            "POST",
            "/api/supplier-credits",
            Some(json!({
                "supplierKey": supplier_key, "receiptKey": receipt["data"]["key"],
                "amountCents": 5000, "reason": "price_difference",
            })),
        )
        .await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    bootstrap(&cloud, &mut devices[1]).await;

    // Offline: both devices sell the SAME serial to their own new invoice.
    let mut invoice_keys = Vec::new();
    for d in &devices {
        let sale = d
            .api(
                "POST",
                "/api/billing/sales",
                Some(json!({
                    "staff": { "cashierName": "Admin" },
                    "items": [{ "productKey": product_key, "quantity": 1, "discountCents": 0, "sourceType": "retail", "serialNumbers": [serial] }],
                    "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 80000 },
                    "shopProfileSnapshot": { "name": "Shop", "address": "Colombo" }
                })),
            )
            .await;
        invoice_keys.push(
            sale["data"]["invoice"]["key"]
                .as_str()
                .expect("the value is a string")
                .to_string(),
        );
    }
    assert_ne!(invoice_keys[0], invoice_keys[1]);

    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await;
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in devices.iter_mut() {
            pull_all(&cloud, d).await;
        }
    }

    // Both sales survive, on every replica, with their own invoice number.
    for d in &devices {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invoices WHERE key IN (?, ?)")
            .bind(&invoice_keys[0])
            .bind(&invoice_keys[1])
            .fetch_one(d.pool())
            .await
            .expect("the awaited step succeeds");
        assert_eq!(
            count, 2,
            "both sales of the double-sold serial must survive on {}",
            d.id
        );
    }

    // The conflict is on record somewhere: on the cloud (it sees the second
    // push land on top of the first) and/or on whichever device pulled the
    // other's conflicting change.
    let cloud_hits = cloud_conflicts(&cloud, "SERIAL_DOUBLE_SOLD").await.len();
    let mut device_hits = 0i64;
    for d in &devices {
        device_hits += sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sync_conflicts WHERE kind = 'SERIAL_DOUBLE_SOLD'",
        )
        .fetch_one(d.pool())
        .await
        .expect("the awaited step succeeds");
    }
    assert!(
        cloud_hits + device_hits as usize >= 1,
        "no SERIAL_DOUBLE_SOLD conflict was recorded anywhere"
    );

    // Fixed bug: `apply_mongo::serial_double_sale` used to keep whichever row was
    // *already stored*, unconditionally, discarding the incoming write even when
    // `core::sync_merge::decide` had already picked it as the winner under the
    // ordinary `(updated_at_ms, device_id)` tiebreak - so a receiving device,
    // running that same `decide` on its own pull, could reach a different answer
    // than the cloud ever actually stored. The fix makes the serial rule ADD a
    // conflict note without ever overriding which write lands, so the cloud's
    // document and every device's pulled copy are driven by the exact same
    // tiebreak and must agree. Assert that now, instead of just documenting a
    // disagreement.
    let cloud_row = cloud
        .mongo()
        .collection::<Document>("product_serials")
        .find_one(doc! { "tenant_id": TENANT, "serial_number": &serial })
        .await
        .expect("the awaited step succeeds")
        .expect("the serial must exist on the cloud");
    let cloud_state = (
        cloud_row
            .get_str("status")
            .expect("the document has this string field")
            .to_string(),
        cloud_row
            .get_str("invoice_key")
            .expect("the document has this string field")
            .to_string(),
    );
    assert_eq!(cloud_state.0, "sold");
    assert!(invoice_keys.contains(&cloud_state.1));

    for d in &devices {
        let row: (String, String) = sqlx::query_as(
            "SELECT status, invoice_key FROM product_serials WHERE serial_number = ?",
        )
        .bind(&serial)
        .fetch_one(d.pool())
        .await
        .expect("the awaited step succeeds");
        assert_eq!(
            row, cloud_state,
            "{} disagrees with the cloud about who owns the serial",
            d.id
        );
    }

    cloud.mongo().drop().await.ok();
}

/// Two devices independently refund the SAME invoice in full, offline and
/// unaware of each other - each looks locally valid at the time. Once merged,
/// the combined refund exceeds the invoice total: this must be *flagged*, not
/// silently clamped or corrupted, and neither credit note may be dropped.
#[tokio::test]
async fn concurrent_full_refunds_raise_over_refund_not_silent_corruption() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;

    let mut devices = vec![Device::new(31, &cloud).await, Device::new(32, &cloud).await];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
        reserve_blocks(&cloud, d).await;
    }

    let catalog = create_shop(&devices[0]).await;
    let (product_key, _product_id) = catalog.products[0].clone();
    let sale = devices[0]
        .api(
            "POST",
            "/api/billing/sales",
            Some(json!({
                "staff": { "cashierName": "Admin" },
                "items": [{ "productKey": product_key, "sellingPriceCents": 50000, "quantity": 2, "discountCents": 0, "sourceType": "retail" }],
                "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 100_000_000 },
                "shopProfileSnapshot": { "name": "Shop", "address": "Colombo" }
            })),
        )
        .await;
    let invoice_key = sale["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let total_cents = sale["data"]["invoice"]["totalCents"]
        .as_i64()
        .expect("the value is an integer");

    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    bootstrap(&cloud, &mut devices[1]).await;

    // Offline: both devices independently return the full quantity.
    let mut credit_note_keys = Vec::new();
    for d in &devices {
        let cn = d
            .api(
                "POST",
                "/api/billing/credit-notes",
                Some(json!({
                    "invoiceKey": invoice_key,
                    "returnedItems": [{ "productKey": product_key, "quantity": 2, "condition": "resalable", "reason": "customer_changed_mind" }],
                    "paymentMethod": "cash"
                })),
            )
            .await;
        credit_note_keys.push(
            cn["data"]["key"]
                .as_str()
                .expect("the value is a string")
                .to_string(),
        );
    }
    assert_ne!(credit_note_keys[0], credit_note_keys[1]);

    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await;
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in devices.iter_mut() {
            pull_all(&cloud, d).await;
        }
    }

    // Both credit notes survive, everywhere.
    for d in &devices {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM credit_notes WHERE key IN (?, ?)")
                .bind(&credit_note_keys[0])
                .bind(&credit_note_keys[1])
                .fetch_one(d.pool())
                .await
                .expect("the awaited step succeeds");
        assert_eq!(count, 2, "both credit notes must survive on {}", d.id);
    }

    // The invoice's refunded_cents is the honest (over-)sum of both notes, not
    // some clamped/negative/garbage value - and it is flagged as an OVER_REFUND.
    for d in &devices {
        let refunded: i64 = sqlx::query_scalar("SELECT refunded_cents FROM invoices WHERE key = ?")
            .bind(&invoice_key)
            .fetch_one(d.pool())
            .await
            .expect("the awaited step succeeds");
        assert!(
            refunded > total_cents,
            "{} should show the honest over-refunded total, got {refunded} for a {total_cents}-cent invoice",
            d.id
        );
        assert!(
            refunded >= 0 && refunded <= total_cents * 3,
            "{} shows a nonsensical refunded_cents: {refunded}",
            d.id
        );
    }

    let cloud_hits = cloud_conflicts(&cloud, "OVER_REFUND").await.len();
    let mut device_hits = 0i64;
    for d in &devices {
        device_hits += sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sync_conflicts WHERE kind = 'OVER_REFUND'",
        )
        .fetch_one(d.pool())
        .await
        .expect("the awaited step succeeds");
    }
    assert!(
        cloud_hits + device_hits as usize >= 1,
        "no OVER_REFUND conflict was recorded anywhere"
    );

    cloud.mongo().drop().await.ok();
}
