//! Shared setup for the `sync_device_test` tests.

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use simplebash_pos_backend::clients::sqlite::map_sqlite_row_to_document;
use simplebash_pos_backend::core::constants::roles;

use simplebash_pos_backend::domain::users::Role;

use crate::common;
use sqlx::SqlitePool;
use tower::ServiceExt;

// Device-side sync (SQLite): capture triggers, outbox, the applier, derived
// recompute, the opening-balance ledger migration, bootstrap and number blocks.
// Two in-process devices talk to each other through the real local sync API,
// exactly the way the desktop shell's agent drives them.

pub(crate) struct Device {
    pub(crate) app: common::TestApp,
    pub(crate) admin: String,
    pub(crate) agent: String,
}

impl Device {
    pub(crate) async fn new() -> Device {
        let app = common::spawn_app_sqlite().await;
        let admin = common::mint_token(
            &app.config,
            Some(Role::Admin),
            roles::default_permissions(Role::Admin),
        );
        let agent = encode(
            &Header::default(),
            &json!({ "sub": "sync-agent", "scope": "sync", "exp": 9_999_999_999u64 }),
            &EncodingKey::from_secret(app.config.jwt_secret.as_bytes()),
        )
        .expect("the token encodes");
        Device { app, admin, agent }
    }

    pub(crate) fn pool(&self) -> &SqlitePool {
        self.app.db_handle.as_sqlite().expect("sqlite device")
    }

    pub(crate) async fn call(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(CONTENT_TYPE, "application/json");
        if let Some(t) = token {
            builder = builder.header(AUTHORIZATION, format!("Bearer {t}"));
        }
        let body = body
            .map(|b| Body::from(serde_json::to_vec(&b).expect("the value serializes to JSON")))
            .unwrap_or_else(Body::empty);
        let response = self
            .app
            .router
            .clone()
            .oneshot(builder.body(body).expect("the value serializes to JSON"))
            .await
            .expect("the awaited step succeeds");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the awaited step succeeds");
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// A normal authenticated API call (as the POS UI would make).
    pub(crate) async fn api(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        let (status, value) = self
            .call(method, uri, body, Some(&self.admin.clone()))
            .await;
        assert!(status.is_success(), "{method} {uri} -> {status}: {value}");
        value
    }

    /// A call to the local sync API with the shell's service token.
    pub(crate) async fn sync(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        let (status, value) = self
            .call(method, uri, body, Some(&self.agent.clone()))
            .await;
        assert!(status.is_success(), "{method} {uri} -> {status}: {value}");
        value["data"].clone()
    }

    pub(crate) async fn state(&self) -> Value {
        self.sync("GET", "/api/sync/state", None).await
    }

    pub(crate) async fn pending_out(&self) -> i64 {
        self.state().await["pendingOut"]
            .as_i64()
            .expect("the value is an integer")
    }

    /// Every outbox record, paged like the agent does.
    pub(crate) async fn drain_outbox(&self) -> Vec<Value> {
        let mut after = 0i64;
        let mut items = Vec::new();
        loop {
            let page = self
                .sync(
                    "GET",
                    &format!("/api/sync/outbox?after={after}&limit=500"),
                    None,
                )
                .await;
            let batch = page["items"]
                .as_array()
                .expect("the value is an array")
                .clone();
            if batch.is_empty() {
                break;
            }
            after = page["lastSeq"].as_i64().expect("the value is an integer");
            items.extend(batch);
        }
        items
    }
}

/// Outbox items as the `changes` of an apply request (what a pull returns).
pub(crate) fn as_pulled(items: &[Value]) -> Vec<Value> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let mut change = item["record"].clone();
            change["seq"] = json!(i as i64 + 1);
            change
        })
        .collect()
}

pub(crate) async fn shop(dev: &Device) -> Value {
    let cat = dev
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
    let sub = dev
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

    let product = dev
        .api(
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": "Phone", "categoryKey": category_key, "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
                "minStockThreshold": 2,
                "openingStock": { "quantity": 10, "unitCostCents": 30000, "sellingPriceCents": 50000 }
            })),
        )
        .await;
    let product_key = product["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let product_id = product["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let customer = dev
        .api("POST", "/api/customers", Some(json!({ "name": "Jane Doe", "primaryPhone": "0771234567", "email": "jane@example.com" })))
        .await;
    let customer_key = customer["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let supplier = dev
        .api(
            "POST",
            "/api/suppliers",
            Some(json!({ "name": "Distributor", "contactPerson": "Sam", "primaryPhone": "0112345678", "address": "Colombo", "suppliedCategories": [category_key] })),
        )
        .await;
    let supplier_key = supplier["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    dev.api(
        "POST",
        "/api/supplier-products",
        Some(json!({ "supplierKey": supplier_key, "productKey": product_key, "costPriceCents": 29000 })),
    )
    .await;
    // A receipt part paid from the bank, with a supplier credit against it: five rows of the new
    // receipt resources (receipt, settlement, credit, and the ledger expense and payment).
    let receipt = dev
        .api(
            "POST",
            "/api/purchase-receipts",
            Some(json!({
                "supplierKey": supplier_key, "date": "2026-09-06T00:00:00Z",
                "lines": [{ "productKey": product_key, "quantity": 5, "lineTotalCents": 145000, "sellingPriceCents": 50000 }],
                "payment": {
                    "source": "bank", "amountCents": 100000, "paidOn": "2026-09-06",
                    "categoryKey": simplebash_pos_backend::domain::cash_categories::category_key(
                        simplebash_pos_backend::domain::sessions::CashMovementType::SupplierPayment,
                        "Stock purchase (cash on delivery)",
                    ),
                },
            })),
        )
        .await;
    dev.api(
        "POST",
        "/api/supplier-credits",
        Some(json!({
            "supplierKey": supplier_key, "receiptKey": receipt["data"]["key"],
            "amountCents": 5000, "reason": "damaged",
        })),
    )
    .await;

    let sale = dev
        .api(
            "POST",
            "/api/billing/sales",
            Some(json!({
                "staff": { "cashierName": "Admin" },
                "customer": { "customerKey": customer_key },
                "items": [{ "productKey": product_key, "sellingPriceCents": 50000, "quantity": 2, "discountCents": 0, "sourceType": "retail" }],
                "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 200000 },
                "shopProfileSnapshot": { "name": "Shop", "address": "Colombo" }
            })),
        )
        .await;
    let invoice_key = sale["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    dev.api(
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{ "productKey": product_key, "quantity": 1, "condition": "resalable", "reason": "defective" }],
            "paymentMethod": "cash"
        })),
    )
    .await;

    json!({
        "categoryKey": category_key, "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } }, "productKey": product_key,
        "productId": product_id, "customerKey": customer_key, "supplierKey": supplier_key, "invoiceKey": invoice_key,
    })
}

/// Table contents with write-metadata removed, JSON columns parsed, for
/// comparing two devices row by row.
pub(crate) async fn table_rows(pool: &SqlitePool, table: &str) -> Vec<Value> {
    let rows = sqlx::query(&format!("SELECT * FROM {table} ORDER BY key"))
        .fetch_all(pool)
        .await
        .expect("the awaited step succeeds");
    rows.iter()
        .map(|row| {
            let mut value = serde_json::to_value(map_sqlite_row_to_document(row))
                .expect("the row converts to JSON");
            let object = value.as_object_mut().expect("the value is an object");
            for meta in ["updated_at", "version", "updated_by_device"] {
                object.remove(meta);
            }
            // Subcategories have no legacy id on the wire (they are joined by key), so
            // each device mints its own.
            if table == "subcategories" {
                object.remove("id");
                object.remove("_id");
            }
            value
        })
        .collect()
}

pub(crate) const SYNCED: &[&str] = &[
    "categories",
    "subcategories",
    "suppliers",
    "products",
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
];

pub(crate) async fn assert_same_data(a: &Device, b: &Device) {
    for table in SYNCED {
        assert_eq!(
            table_rows(a.pool(), table).await,
            table_rows(b.pool(), table).await,
            "table {table} differs"
        );
    }
}

// ---------------------------------------------------------------------------
// Users (P3b): logins travel with the shop
// ---------------------------------------------------------------------------

/// Creates a staff login on `dev` and returns `(id, key)`.
pub(crate) async fn create_cashier(
    dev: &Device,
    username: &str,
    password: &str,
) -> (String, String) {
    let created = dev
        .api("POST", "/api/users", Some(json!({ "name": "Cashier One", "username": username, "password": password, "role": "staff" })))
        .await;
    (
        created["data"]["id"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
        created["data"]["key"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
    )
}

pub(crate) async fn login_status(dev: &Device, username: &str, password: &str) -> StatusCode {
    dev.call(
        "POST",
        "/api/auth/login",
        Some(json!({ "username": username, "password": password })),
        None,
    )
    .await
    .0
}

pub(crate) async fn apply(dev: &Device, changes: Vec<Value>) -> Value {
    let applied = dev
        .sync(
            "POST",
            "/api/sync/apply",
            Some(json!({ "mode": "incremental", "changes": changes })),
        )
        .await;
    assert_eq!(applied["conflicts"], 0, "{applied}");
    applied
}
