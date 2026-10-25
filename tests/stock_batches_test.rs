// End-to-end coverage of stock batches — every delivery of a product with
// its own cost, selling price and remaining quantity — through the real
// router. The scenario is the one the feature was built for: item A bought
// 10 @ Rs.1000 and sold @ Rs.1200, then 12 more bought @ Rs.1100 and sold @
// Rs.1500, the cashier picking the price at the till. It runs unchanged on
// both engines: SQLite (the desktop) and a throwaway Atlas database (the
// cloud), dropped at the end.

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use serde_json::{Value, json};
use simplebash_pos_backend::{core::constants::roles, domain::users::Role};
use tower::ServiceExt;

async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json)
}

struct Shop {
    router: axum::Router,
    admin: String,
    manager: String,
    supplier_key: String,
    product_id: String,
    product_key: String,
}

impl Shop {
    async fn new(app: &common::TestApp) -> Shop {
        let router = app.router.clone();
        let admin = common::mint_token(
            &app.config,
            Some(Role::Admin),
            roles::default_permissions(Role::Admin),
        );
        let manager = common::mint_token(
            &app.config,
            Some(Role::Manager),
            roles::default_permissions(Role::Manager),
        );
        let tag = &uuid::Uuid::new_v4().simple().to_string()[..8];

        let (status, category) = send(
            &router,
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": format!("Batch Cat {tag}"), "icon": "box", "color": "blue" })),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{category}");
        let category_key = category["data"]["key"].as_str().unwrap().to_string();
        let (status, sub) = send(
            &router,
            "POST",
            &format!("/api/inventory/categories/{category_key}/subcategories"),
            Some(json!({ "name": "Items" })),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{sub}");
        let subcategory_key = sub["data"]["subcategories"][0]["key"]
            .as_str()
            .unwrap()
            .to_string();

        let (status, supplier) = send(
            &router,
            "POST",
            "/api/suppliers",
            Some(json!({
                "name": format!("Batch Supplier {tag}"),
                "contactPerson": "Nimal",
                "primaryPhone": "0771234567",
                "address": "Colombo",
                "suppliedCategories": [category_key],
            })),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{supplier}");

        // A product carries no price of its own until stock arrives.
        let (status, product) = send(
            &router,
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": format!("Item A {tag}"),
                "categoryKey": category_key,
                "subcategoryKey": subcategory_key,
                "minStockThreshold": 2,
            })),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{product}");
        assert_eq!(product["data"]["stockQuantity"], 0);
        assert_eq!(product["data"]["priceTiers"], json!([]));

        Shop {
            router,
            admin,
            manager,
            supplier_key: supplier["data"]["key"].as_str().unwrap().to_string(),
            product_id: product["data"]["id"].as_str().unwrap().to_string(),
            product_key: product["data"]["key"].as_str().unwrap().to_string(),
        }
    }

    async fn purchase(&self, quantity: i64, cost: i64, price: i64, date: &str) {
        let (status, body) = send(
            &self.router,
            "POST",
            "/api/purchase-receipts",
            Some(json!({
                "supplierKey": self.supplier_key,
                "date": date,
                "lines": [{
                    "productKey": self.product_key,
                    "quantity": quantity,
                    "unitCostCents": cost,
                    "sellingPriceCents": price,
                }],
            })),
            &self.admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["data"]["lines"][0]["sellingPriceCents"], price);
    }

    async fn sell(&self, lines: &[(i64, i64)]) -> (StatusCode, Value) {
        let items: Vec<Value> = lines
            .iter()
            .map(|(price, quantity)| {
                json!({
                    "productKey": self.product_key,
                    "sellingPriceCents": price,
                    "quantity": quantity,
                    "discountCents": 0,
                    "sourceType": "retail",
                })
            })
            .collect();
        send(
            &self.router,
            "POST",
            "/api/billing/sales",
            Some(json!({
                "staff": { "cashierName": "Cashier" },
                "items": items,
                "payment": { "paymentMethod": "cash", "isCredit": false },
                "shopProfileSnapshot": { "name": "Batch Shop" },
            })),
            &self.admin,
        )
        .await
    }

    async fn product(&self) -> Value {
        let (status, body) = send(
            &self.router,
            "GET",
            &format!("/api/inventory/products/{}", self.product_id),
            None,
            &self.admin,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"].clone()
    }

    /// `(selling price, quantity)` per tier, cheapest first.
    async fn tiers(&self) -> Vec<(i64, i64)> {
        self.product().await["priceTiers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                (
                    t["sellingPriceCents"].as_i64().unwrap(),
                    t["quantity"].as_i64().unwrap(),
                )
            })
            .collect()
    }

    /// Batches newest first, as `GET /products/{key}/batches` lists them.
    async fn batches(&self, token: &str) -> Vec<Value> {
        let (status, body) = send(
            &self.router,
            "GET",
            &format!("/api/inventory/products/{}/batches", self.product_key),
            None,
            token,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"]["items"].as_array().unwrap().clone()
    }

    /// Remaining quantity per batch key.
    async fn remaining(&self) -> std::collections::HashMap<String, i64> {
        self.batches(&self.admin)
            .await
            .iter()
            .map(|b| {
                (
                    b["key"].as_str().unwrap().to_string(),
                    b["remainingQuantity"].as_i64().unwrap(),
                )
            })
            .collect()
    }
}

async fn item_a_scenario(app: &common::TestApp) {
    let shop = Shop::new(app).await;

    // First delivery: 10 @ Rs.1000, sold @ Rs.1200.
    shop.purchase(10, 100_000, 120_000, "2026-10-01T00:00:00Z")
        .await;
    assert_eq!(shop.tiers().await, vec![(120_000, 10)]);
    let (status, body) = shop.sell(&[(120_000, 5)]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(shop.tiers().await, vec![(120_000, 5)]);

    // Second delivery: 12 @ Rs.1100, sold @ Rs.1500. Both prices are now on offer.
    shop.purchase(12, 110_000, 150_000, "2026-10-02T00:00:00Z")
        .await;
    assert_eq!(shop.tiers().await, vec![(120_000, 5), (150_000, 12)]);
    let batches = shop.batches(&shop.admin).await;
    let first = batches[1]["key"].as_str().unwrap().to_string();
    let second = batches[0]["key"].as_str().unwrap().to_string();
    assert_eq!(batches[0]["sellingPriceCents"], 150_000);
    assert_eq!(batches[0]["source"], "purchase");
    assert!(batches[0]["purchaseKey"].is_string());

    // One cart, one line per price: each line priced and costed by its own delivery.
    let (status, sale) = shop.sell(&[(120_000, 2), (150_000, 3)]).await;
    assert_eq!(status, StatusCode::OK, "{sale}");
    assert_eq!(sale["data"]["warnings"], json!([]));
    let items = &sale["data"]["invoice"]["items"];
    assert_eq!(items[0]["unitPriceCents"], 120_000);
    assert_eq!(items[0]["unitCostCents"], 100_000);
    assert_eq!(items[0]["batchAllocations"][0]["batchKey"], first);
    assert_eq!(items[1]["unitPriceCents"], 150_000);
    assert_eq!(items[1]["unitCostCents"], 110_000);
    assert_eq!(items[1]["batchAllocations"][0]["batchKey"], second);
    assert_eq!(
        sale["data"]["invoice"]["totalCents"],
        2 * 120_000 + 3 * 150_000
    );
    let voidable_invoice = sale["data"]["invoice"]["key"].as_str().unwrap().to_string();
    assert_eq!(shop.tiers().await, vec![(120_000, 3), (150_000, 9)]);
    assert_eq!(shop.product().await["stockQuantity"], 12);

    // A tier can't be oversold, a price nobody sells at is refused, and a
    // product line must name its price — all before anything is written.
    let (status, body) = shop.sell(&[(120_000, 4)]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "INSUFFICIENT_STOCK");
    let (status, body) = shop.sell(&[(130_000, 1)]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "PRICE_TIER_UNAVAILABLE");
    let (status, body) = send(
        &shop.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier" },
            "items": [{ "productKey": shop.product_key, "quantity": 1, "discountCents": 0, "sourceType": "retail" }],
            "payment": { "paymentMethod": "cash", "isCredit": false },
                "shopProfileSnapshot": { "name": "Batch Shop" },
        })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = shop.sell(&[(150_000, 1), (150_000, 1)]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(shop.product().await["stockQuantity"], 12);

    // A third delivery at the same Rs.1500 merges into that tier; a sale
    // from it takes the oldest delivery first and splits across both.
    shop.purchase(4, 105_000, 150_000, "2026-10-03T00:00:00Z")
        .await;
    let third = shop.batches(&shop.admin).await[0]["key"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(shop.tiers().await, vec![(120_000, 3), (150_000, 13)]);
    let (status, split_sale) = shop.sell(&[(150_000, 11)]).await;
    assert_eq!(status, StatusCode::OK, "{split_sale}");
    let line = &split_sale["data"]["invoice"]["items"][0];
    assert_eq!(
        line["batchAllocations"],
        json!([
            { "batchKey": second, "quantity": 9, "unitCostCents": 110_000 },
            { "batchKey": third, "quantity": 2, "unitCostCents": 105_000 },
        ])
    );
    assert_eq!(line["unitCostCents"], (9 * 110_000 + 2 * 105_000) / 11);
    let remaining = shop.remaining().await;
    assert_eq!((remaining[&second], remaining[&third]), (0, 2));

    // Returning 3 of that line puts them back newest delivery first.
    let (status, credit_note) = send(
        &shop.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": split_sale["data"]["invoice"]["key"],
            "returnedItems": [{
                "productKey": shop.product_key, "quantity": 3,
                "condition": "resalable", "reason": "defective",
            }],
            "paymentMethod": "cash",
        })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{credit_note}");
    let remaining = shop.remaining().await;
    assert_eq!((remaining[&second], remaining[&third]), (1, 4));

    // Voiding the mixed sale restores each line's own delivery.
    let (status, body) = send(
        &shop.router,
        "POST",
        &format!("/api/billing/invoices/{voidable_invoice}/void"),
        Some(json!({ "reason": "Entered by mistake" })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let remaining = shop.remaining().await;
    assert_eq!((remaining[&first], remaining[&second]), (5, 4));

    // A no-receipt return at Rs.1200 rejoins the Rs.1200 delivery.
    let (status, body) = send(
        &shop.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "noReceipt": true,
            "overrideReason": "Customer lost receipt",
            "returnedItems": [{
                "productKey": shop.product_key, "quantity": 1, "sellingPriceCents": 120_000,
                "condition": "resalable", "reason": "defective",
            }],
            "paymentMethod": "cash",
        })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["returnedItems"][0]["unitPriceCents"], 120_000);
    assert_eq!(shop.remaining().await[&first], 6);

    // Re-pricing a delivery moves its whole tier; only an Admin sees or edits cost.
    let (status, body) = send(
        &shop.router,
        "PATCH",
        &format!("/api/inventory/batches/{first}"),
        Some(json!({ "sellingPriceCents": 125_000 })),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"].get("unitCostCents").is_none());
    let (status, body) = send(
        &shop.router,
        "PATCH",
        &format!("/api/inventory/batches/{first}"),
        Some(json!({ "unitCostCents": 1 })),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(
        shop.batches(&shop.manager)
            .await
            .iter()
            .all(|b| b.get("unitCostCents").is_none())
    );
    assert_eq!(shop.tiers().await, vec![(125_000, 6), (150_000, 8)]);

    // The product total is always the sum of its batches, and the inventory
    // report values every unit at its own batch's cost and price.
    let batches = shop.batches(&shop.admin).await;
    let remaining_total: i64 = batches
        .iter()
        .map(|b| b["remainingQuantity"].as_i64().unwrap())
        .sum();
    assert_eq!(shop.product().await["stockQuantity"], remaining_total);
    let (status, report) = send(
        &shop.router,
        "GET",
        "/api/reports/inventory-valuation",
        None,
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    let cost_value: i64 = batches
        .iter()
        .map(|b| b["remainingQuantity"].as_i64().unwrap() * b["unitCostCents"].as_i64().unwrap())
        .sum();
    assert_eq!(report["data"]["totalCostValuationCents"], cost_value);
}

#[tokio::test]
async fn item_a_scenario_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    item_a_scenario(&app).await;
}

#[tokio::test]
async fn item_a_scenario_on_mongo() {
    let app = common::spawn_app_isolated().await;
    item_a_scenario(&app).await;
    app.db.drop().await.ok();
}

/// A desktop database from before stock batches (prices on the product row,
/// movements/serials/sale lines without batches) is migrated on startup: the
/// product's stock and price move into its opening batch, history points at
/// it, the price columns are gone, and a second start changes nothing.
#[tokio::test]
async fn a_sqlite_database_from_before_batches_is_migrated() {
    use simplebash_pos_backend::clients::sqlite;
    use sqlx::Executor;

    let path = std::env::temp_dir().join(format!("pos_batch_mig_{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlite::connect(&url).await.unwrap();

    // Recreate the old shape on top of a fresh schema.
    for statement in [
        "ALTER TABLE products ADD COLUMN cost_price_cents INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE products ADD COLUMN selling_price_cents INTEGER NOT NULL DEFAULT 0",
        "INSERT INTO products (key, id, sku, name, category_key, subcategory_key, cost_price_cents, \
         selling_price_cents, stock_quantity, version, created_at, updated_at) VALUES \
         ('prod_old', '65f1a1a1a1a1a1a1a1a1a1c1', 'OLD-1', 'Old Item', 'cat_1', 'subcat_1', 1000, 1200, 5, 1, \
         '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
        "INSERT INTO stock_movements (key, id, product_id, quantity_delta, movement_type, version, created_at, updated_at) \
         VALUES ('sm_old', '65f1a1a1a1a1a1a1a1a1a1c2', '65f1a1a1a1a1a1a1a1a1a1c1', 5, 'opening_balance', 1, \
         '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
        "INSERT INTO stock_movements (key, id, product_id, quantity_delta, movement_type, version, created_at, updated_at) \
         VALUES ('sm_audit', '65f1a1a1a1a1a1a1a1a1a1c3', '65f1a1a1a1a1a1a1a1a1a1c1', 0, 'return_write_off', 1, \
         '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
        "INSERT INTO product_serials (key, id, product_key, serial_number, batch_key, status, version, created_at, updated_at) \
         VALUES ('psn_old', '65f1a1a1a1a1a1a1a1a1a1c4', 'prod_old', 'SN-OLD-1', '', 'in_stock', 1, \
         '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
        "INSERT INTO invoices (key, id, invoice_number, cashier_id, cashier_name_snapshot, items, subtotal_cents, \
         total_cents, payment_method, status, shop_profile_snapshot, created_at, updated_at) VALUES \
         ('inv_old', '65f1a1a1a1a1a1a1a1a1a1c5', 'INV-OLD-1', 'usr_1', 'Cashier', \
         '[{\"productKey\":\"prod_old\",\"name\":\"Old Item\",\"unitPriceCents\":1200,\"quantity\":2,\"discountCents\":0,\"totalCents\":2400,\"unitCostCents\":900,\"sourceType\":\"retail\",\"returnedQuantity\":0,\"serialNumbers\":[]}]', \
         2400, 2400, 'cash', 'paid', '{}', '2026-01-02T00:00:00+00:00', '2026-01-02T00:00:00+00:00')",
    ] {
        pool.execute(statement).await.unwrap();
    }

    sqlite::init_db(&pool).await.unwrap();

    let batch: (String, String, i64, i64, i64) = sqlx::query_as(
        "SELECT key, id, remaining_quantity, unit_cost_cents, selling_price_cents FROM stock_batches WHERE product_key = 'prod_old'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        batch,
        (
            "stb_open_prod_old".into(),
            "65f1a1a1a1a1a1a1a1a1a1c1".into(),
            5,
            1000,
            1200
        )
    );
    let movements: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT key, batch_key FROM stock_movements ORDER BY key")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        movements,
        vec![
            ("sm_audit".into(), None),
            ("sm_old".into(), Some("stb_open_prod_old".into()))
        ]
    );
    let serial: (String,) =
        sqlx::query_as("SELECT batch_key FROM product_serials WHERE key = 'psn_old'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(serial.0, "stb_open_prod_old");
    let items: (String,) = sqlx::query_as("SELECT items FROM invoices WHERE key = 'inv_old'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let items: Value = serde_json::from_str(&items.0).unwrap();
    assert_eq!(
        items[0]["batchAllocations"],
        json!([{ "batchKey": "stb_open_prod_old", "quantity": 2, "unitCostCents": 900 }])
    );
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('products')")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(!columns.iter().any(|c| c.contains("price")));
    // The capture triggers come back after the migration dropped them.
    let triggers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = 'sync_stock_batches_upd'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(triggers, 1);

    // Idempotent: a second start finds nothing to migrate.
    sqlite::init_db(&pool).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_batches")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    pool.close().await;
    let _ = std::fs::remove_file(&path);
}

/// The cloud migration (`stock_batch_migration::migrate_mongo`) turns
/// old-shape documents into the same opening batch the desktop migration
/// creates, rewrites history to point at it, is idempotent, and leaves the
/// product sellable at its old price through the real API.
#[tokio::test]
async fn a_mongo_database_from_before_batches_is_migrated() {
    use mongodb::bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId};
    use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::{
        Mode, SINGLE_SHOP, migrate_mongo,
    };

    let app = common::spawn_app_isolated().await;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let product_id = ObjectId::new();
    let created = BsonDateTime::from_millis(1_767_225_600_000);
    let insert = |collection: &str, document: Document| {
        let db = app.db.clone();
        let collection = collection.to_string();
        async move {
            db.collection::<Document>(&collection)
                .insert_one(document)
                .await
                .unwrap();
        }
    };
    insert(
        "products",
        doc! {
            "_id": product_id, "key": "prod_old", "sku": "OLD-1", "name": "Old Item",
            "category_key": "cat_1", "subcategory_key": "subcat_1",
            "cost_price_cents": 1000_i64, "selling_price_cents": 1200_i64,
            "stock_quantity": 5_i64, "min_stock_threshold": 0_i64, "is_serialized": false,
            "version": 1_i64, "created_at": created, "updated_at": created,
        },
    )
    .await;
    insert(
        "stock_movements",
        doc! {
            "key": "sm_old", "product_id": product_id, "quantity_delta": 5_i64,
            "movement_type": "opening_balance", "version": 1_i64,
            "created_at": created, "updated_at": created,
        },
    )
    .await;
    insert(
        "purchases",
        doc! {
            "key": "pur_old", "supplier_key": "sup_1", "product_key": "prod_old",
            "quantity": 5_i64, "unit_cost_cents": 1000_i64, "date": created,
            "version": 1_i64, "created_at": created, "updated_at": created,
        },
    )
    .await;
    insert(
        "invoices",
        doc! {
            "key": "inv_old", "items": [{
                "productKey": "prod_old", "name": "Old Item", "unitPriceCents": 1200_i64,
                "quantity": 2_i64, "discountCents": 0_i64, "totalCents": 2400_i64,
                "sourceType": "retail", "returnedQuantity": 0_i64, "serialNumbers": [],
            }],
        },
    )
    .await;

    // A second shop's product selling at zero, and an old line whose product
    // no longer exists and that never recorded a cost: both are reported.
    insert(
        "products",
        doc! {
            "_id": ObjectId::new(), "key": "prod_zero", "sku": "ZERO-1", "name": "Free Item",
            "category_key": "cat_1", "subcategory_key": "subcat_1", "tenant_id": "tnt_b",
            "cost_price_cents": 0_i64, "selling_price_cents": 0_i64,
            "stock_quantity": 0_i64, "min_stock_threshold": 0_i64, "is_serialized": false,
            "version": 1_i64, "created_at": created, "updated_at": created,
        },
    )
    .await;
    insert(
        "invoices",
        doc! {
            "key": "inv_gone", "tenant_id": "tnt_b", "items": [{
                "productKey": "prod_gone", "name": "Gone", "unitPriceCents": 500_i64,
                "quantity": 1_i64, "discountCents": 0_i64, "totalCents": 500_i64,
                "sourceType": "retail", "returnedQuantity": 0_i64, "serialNumbers": [],
            }],
        },
    )
    .await;

    // A dry run reports per shop what would change and writes nothing.
    let dry = migrate_mongo(&app.db, Mode::DryRun).await.unwrap();
    let shop_a = &dry.shops[SINGLE_SHOP];
    assert_eq!(
        (shop_a.products, shop_a.batches, shop_a.movements),
        (1, 1, 1)
    );
    assert_eq!(
        (
            shop_a.purchases,
            shop_a.invoices,
            shop_a.zero_price_products
        ),
        (1, 1, 0)
    );
    let shop_b = &dry.shops["tnt_b"];
    assert_eq!(
        (shop_b.products, shop_b.batches, shop_b.zero_price_products),
        (1, 1, 1)
    );
    assert_eq!((shop_b.invoices, shop_b.lines_without_cost), (0, 1));
    assert_eq!(dry.total().products, 2);
    assert_eq!(
        app.db
            .collection::<Document>("stock_batches")
            .count_documents(doc! {})
            .await
            .unwrap(),
        0
    );
    let untouched = app
        .db
        .collection::<Document>("products")
        .find_one(doc! { "_id": product_id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(untouched.get_i64("selling_price_cents").unwrap(), 1200);
    let untouched_movement = app
        .db
        .collection::<Document>("stock_movements")
        .find_one(doc! { "key": "sm_old" })
        .await
        .unwrap()
        .unwrap();
    assert!(!untouched_movement.contains_key("batch_key"));

    // The real run does exactly what the dry run said.
    let first = migrate_mongo(&app.db, Mode::Apply).await.unwrap();
    assert_eq!(first.shops, dry.shops);

    let batch = app
        .db
        .collection::<Document>("stock_batches")
        .find_one(doc! { "key": "stb_open_prod_old" })
        .await
        .unwrap()
        .expect("opening batch");
    assert_eq!(batch.get_object_id("_id").unwrap(), product_id);
    assert_eq!(batch.get_i64("remaining_quantity").unwrap(), 5);
    assert_eq!(batch.get_i64("selling_price_cents").unwrap(), 1200);
    let movement = app
        .db
        .collection::<Document>("stock_movements")
        .find_one(doc! { "key": "sm_old" })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(movement.get_str("batch_key").unwrap(), "stb_open_prod_old");
    let invoice = app
        .db
        .collection::<Document>("invoices")
        .find_one(doc! { "key": "inv_old" })
        .await
        .unwrap()
        .unwrap();
    let line = invoice.get_array("items").unwrap()[0]
        .as_document()
        .unwrap()
        .clone();
    let allocation = line.get_array("batchAllocations").unwrap()[0]
        .as_document()
        .unwrap()
        .clone();
    assert_eq!(allocation.get_str("batchKey").unwrap(), "stb_open_prod_old");
    assert_eq!(allocation.get_i64("unitCostCents").unwrap(), 1000);
    let product = app
        .db
        .collection::<Document>("products")
        .find_one(doc! { "_id": product_id })
        .await
        .unwrap()
        .unwrap();
    assert!(!product.contains_key("selling_price_cents"));

    let again = migrate_mongo(&app.db, Mode::Apply).await.unwrap();
    let again = again.total();
    assert_eq!((again.products, again.batches, again.invoices), (0, 0, 0));

    let (status, body) = send(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{}", product_id.to_hex()),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["priceTiers"][0]["sellingPriceCents"], 1200);
    assert_eq!(body["data"]["priceTiers"][0]["quantity"], 5);

    app.db.drop().await.ok();
}

/// A product sold at two prices sits on two invoice lines. A return names the
/// price, so it lands on the right line and its own delivery; voiding the
/// return gives the count back to that same line only.
async fn two_price_return_scenario(app: &common::TestApp) {
    let shop = Shop::new(app).await;
    shop.purchase(10, 100_000, 120_000, "2026-10-01T00:00:00Z")
        .await;
    shop.purchase(12, 110_000, 150_000, "2026-10-02T00:00:00Z")
        .await;
    let batches = shop.batches(&shop.admin).await;
    let (dearer, cheaper) = (
        batches[0]["key"].as_str().unwrap().to_string(),
        batches[1]["key"].as_str().unwrap().to_string(),
    );

    let (status, sale) = shop.sell(&[(120_000, 2), (150_000, 3)]).await;
    assert_eq!(status, StatusCode::OK, "{sale}");
    let invoice_key = sale["data"]["invoice"]["key"].as_str().unwrap().to_string();

    let (status, credit_note) = send(
        &shop.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": shop.product_key, "sellingPriceCents": 150_000, "quantity": 2,
                "condition": "resalable", "reason": "defective",
            }],
            "paymentMethod": "cash",
        })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{credit_note}");
    assert_eq!(credit_note["data"]["returnSubtotalCents"], 2 * 150_000);
    assert_eq!(
        credit_note["data"]["returnedItems"][0]["batchAllocations"][0]["batchKey"],
        dearer
    );
    let remaining = shop.remaining().await;
    assert_eq!((remaining[&cheaper], remaining[&dearer]), (8, 11));

    let invoice_lines = |body: &Value| -> Vec<i64> {
        body["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line["returnedQuantity"].as_i64().unwrap())
            .collect()
    };
    let (_, invoice) = send(
        &shop.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}"),
        None,
        &shop.admin,
    )
    .await;
    assert_eq!(invoice_lines(&invoice), vec![0, 2]);

    // A price the invoice never sold the product at matches no line.
    let (status, body) = send(
        &shop.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": shop.product_key, "sellingPriceCents": 130_000, "quantity": 1,
                "condition": "resalable", "reason": "defective",
            }],
            "paymentMethod": "cash",
        })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, body) = send(
        &shop.router,
        "POST",
        &format!(
            "/api/billing/credit-notes/{}/void",
            credit_note["data"]["key"].as_str().unwrap()
        ),
        Some(json!({ "reason": "Returned by mistake" })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, invoice) = send(
        &shop.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}"),
        None,
        &shop.admin,
    )
    .await;
    assert_eq!(invoice_lines(&invoice), vec![0, 0]);
    let remaining = shop.remaining().await;
    assert_eq!((remaining[&cheaper], remaining[&dearer]), (8, 9));
}

#[tokio::test]
async fn two_price_return_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    two_price_return_scenario(&app).await;
}

#[tokio::test]
async fn two_price_return_on_mongo() {
    let app = common::spawn_app_isolated().await;
    two_price_return_scenario(&app).await;
    app.db.drop().await.ok();
}
