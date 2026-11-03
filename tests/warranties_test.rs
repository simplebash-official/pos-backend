//! Warranty records end to end, on SQLite and on a throwaway Atlas database: issued per sold
//! serial and per non-serialized line at checkout, the till override and its permission, the
//! counter lookup by every kind of text, the detail with its log, the manager-approved void and
//! extend, and the automatic cancellation on a return and on a voided invoice.

mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::middleware::auth::Claims;
use simplebash_pos_backend::domain::users::Role;
use tower::ServiceExt;

const MANAGER_PIN: &str = "7391";
const STAFF_PIN: &str = "2468";

async fn send(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .expect("the request is well formed");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body reads");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("the body is JSON")
    };
    (status, json)
}

fn token_for(app: &common::TestApp, user: &str, role: Role, name: &str) -> String {
    let claims = Claims {
        sub: user.to_string(),
        exp: 9_999_999_999,
        role: Some(role),
        permissions: roles::default_permissions(role)
            .iter()
            .map(|p| p.to_string())
            .collect(),
        tid: None,
        scope: None,
        did: None,
        name: Some(name.to_string()),
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(app.config.jwt_secret.as_bytes()),
    )
    .expect("the token encodes")
}

fn text(value: &Value) -> String {
    value.as_str().expect("the value is a string").to_string()
}

fn items(body: &Value) -> Vec<Value> {
    body["data"]["items"]
        .as_array()
        .expect("the value is an array")
        .clone()
}

/// A user of `role` with `pin`, and a token for them.
async fn user_with_pin(
    app: &common::TestApp,
    admin: &str,
    name: &str,
    role: (Role, &str),
    pin: &str,
) -> String {
    let (status, body) = send(
        app,
        "POST",
        "/api/users",
        Some(json!({
            "name": name,
            "username": format!("{name}_{}", &uuid::Uuid::new_v4().simple().to_string()[..6]),
            "password": "Password123!",
            "role": role.1,
        })),
        admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let token = token_for(app, &text(&body["data"]["id"]), role.0, name);
    let (status, body) = send(
        app,
        "POST",
        "/api/auth/set-pin",
        Some(json!({ "pin": pin })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    token
}

async fn product(app: &common::TestApp, admin: &str, body: Value) -> String {
    let (status, created) = send(app, "POST", "/api/inventory/products", Some(body), admin).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    text(&created["data"]["key"])
}

fn sale(lines: Value) -> Value {
    json!({
        "staff": { "cashierName": "Cashier Kamal" },
        "customer": { "customerName": "Nimal Perera", "customerPhone": "077 123 4567" },
        "items": lines,
        "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 10_000_000 },
        "shopProfileSnapshot": { "tradingName": "Warranty Test Shop" },
    })
}

async fn warranties_of(app: &common::TestApp, token: &str, invoice_key: &str) -> Vec<Value> {
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/warranties?invoiceKey={invoice_key}"),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    items(&body)
}

async fn lookup(app: &common::TestApp, token: &str, q: &str) -> Value {
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/warranties/lookup?q={}", urlencode(q)),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

fn urlencode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn days_between(from: &str, to: &str) -> i64 {
    let parse = |d: &str| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").expect("a date");
    (parse(to) - parse(from)).num_days()
}

async fn scenario(app: &common::TestApp) {
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the admin is seeded");
    let admin = token_for(app, "admin", Role::Admin, "Owner Olivia");
    let manager = user_with_pin(app, &admin, "mgr", (Role::Manager, "manager"), MANAGER_PIN).await;
    let staff = user_with_pin(app, &admin, "kamal", (Role::Staff, "staff"), STAFF_PIN).await;
    let _ = manager;

    // Catalogue: a serialized phone with a shop warranty, a charger with a manufacturer one,
    // and a cable with none.
    let (status, category) = send(
        app,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Phones {}", uuid::Uuid::new_v4()), "icon": "Box", "color": "blue",
            "subcategories": ["Smartphones"],
            "warrantyTemplate": {
                "coverage": { "kind": "limited", "value": 12, "unit": "months" },
                "warrantyType": "shop",
            },
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{category}");
    assert_eq!(
        category["data"]["warrantyTemplate"]["coverage"]["value"],
        12
    );
    let category_key = text(&category["data"]["key"]);
    let subcategory_key = text(&category["data"]["subcategories"][0]["key"]);

    let base = |name: &str| {
        json!({
            "name": format!("{name} {}", &uuid::Uuid::new_v4().simple().to_string()[..6]),
            "categoryKey": category_key, "subcategoryKey": subcategory_key,
            "minStockThreshold": 1,
        })
    };
    let mut phone_body = base("Phone");
    phone_body["isSerialized"] = json!(true);
    phone_body["warranty"] = json!({
        "coverage": { "kind": "limited", "value": 12, "unit": "months" },
        "warrantyType": "shop",
        "terms": "  Screen damage is not covered.  ",
    });
    let phone = product(app, &admin, phone_body).await;
    let mut charger_body = base("Charger");
    charger_body["openingStock"] =
        json!({ "quantity": 10, "unitCostCents": 500, "sellingPriceCents": 1500 });
    charger_body["warranty"] = json!({
        "coverage": { "kind": "limited", "value": 6, "unit": "months" },
        "warrantyType": "manufacturer",
    });
    let charger = product(app, &admin, charger_body).await;
    let mut cable_body = base("Cable");
    cable_body["openingStock"] =
        json!({ "quantity": 10, "unitCostCents": 100, "sellingPriceCents": 500 });
    cable_body["warranty"] = json!({ "coverage": { "kind": "none" } });
    let cable = product(app, &admin, cable_body).await;

    // A covered product needs a type; an uncovered one cannot have one.
    let mut bad = base("Bad");
    bad["warranty"] = json!({ "coverage": { "kind": "limited", "value": 3, "unit": "days" } });
    let (status, body) = send(app, "POST", "/api/inventory/products", Some(bad), &admin).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let mut bad = base("Bad");
    bad["warranty"] = json!({ "coverage": { "kind": "none" }, "warrantyType": "shop" });
    let (status, body) = send(app, "POST", "/api/inventory/products", Some(bad), &admin).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // Two phones arrive.
    let (status, supplier) = send(
        app,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": format!("Supplier {}", uuid::Uuid::new_v4()), "contactPerson": "Jane",
            "primaryPhone": "+94770000001", "address": "Colombo", "suppliedCategories": ["Phones"],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{supplier}");
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let serial_a = format!("IMEI{}", &tag[..11]);
    let serial_b = format!("IMEI{}", &tag[11..22]);
    let (status, body) = send(
        app,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": text(&supplier["data"]["key"]),
            "date": chrono::Utc::now().to_rfc3339(),
            "lines": [{
                "productKey": phone, "quantity": 2, "lineTotalCents": 100_000,
                "sellingPriceCents": 90_000, "serialNumbers": [serial_a, serial_b],
            }],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // The sale: two phones, three chargers and a cable.
    let (status, sold) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(sale(json!([
            { "productKey": phone, "quantity": 2, "discountCents": 0, "sourceType": "retail",
              "serialNumbers": [serial_a, serial_b] },
            { "productKey": charger, "quantity": 3, "discountCents": 0, "sourceType": "retail",
              "sellingPriceCents": 1500 },
            { "productKey": cable, "quantity": 1, "discountCents": 0, "sourceType": "retail",
              "sellingPriceCents": 500 },
        ]))),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    assert_eq!(sold["message"], "Sale completed successfully", "{sold}");
    let invoice_key = text(&sold["data"]["invoice"]["key"]);
    let invoice_number = text(&sold["data"]["invoice"]["invoiceNumber"]);

    let issued = warranties_of(app, &staff, &invoice_key).await;
    assert_eq!(
        issued.len(),
        3,
        "one per phone and one for the charger line: {issued:?}"
    );
    let phone_a = issued
        .iter()
        .find(|w| w["serialNumber"] == serial_a.as_str())
        .expect("phone A has a warranty")
        .clone();
    let charger_w = issued
        .iter()
        .find(|w| w["serialNumber"].is_null())
        .expect("the charger line has a warranty")
        .clone();
    assert_eq!(phone_a["warrantyType"], "shop");
    assert_eq!(phone_a["terms"], "Screen damage is not covered.");
    assert_eq!(phone_a["quantity"], 1);
    assert_eq!(phone_a["lineIndex"], 0);
    assert_eq!(phone_a["unitPriceCents"], 90_000);
    assert_eq!(phone_a["customerName"], "Nimal Perera");
    assert_eq!(phone_a["state"], "active");
    assert!(text(&phone_a["warrantyNumber"]).starts_with("WTY-"));
    let starts = text(&phone_a["startsOn"]);
    let ends = text(&phone_a["endsOn"]);
    assert!(
        (364..=365).contains(&days_between(&starts, &ends)),
        "{starts} {ends}"
    );
    assert_eq!(charger_w["quantity"], 3);
    assert_eq!(charger_w["warrantyType"], "manufacturer");
    assert_eq!(charger_w["lineIndex"], 1);

    // Changing a line's warranty at the till needs the permission.
    let override_sale = sale(json!([
        { "productKey": cable, "quantity": 1, "discountCents": 0, "sourceType": "retail",
          "sellingPriceCents": 500,
          "warrantyOverride": {
              "coverage": { "kind": "limited", "value": 7, "unit": "days" },
              "warrantyType": "shop",
          } },
        { "productKey": charger, "quantity": 1, "discountCents": 0, "sourceType": "retail",
          "sellingPriceCents": 1500, "warrantyOverride": { "coverage": { "kind": "none" } } },
    ]));
    let (status, body) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(override_sale.clone()),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, overridden) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(override_sale),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{overridden}");
    let override_invoice = text(&overridden["data"]["invoice"]["key"]);
    let override_warranties = warranties_of(app, &staff, &override_invoice).await;
    assert_eq!(override_warranties.len(), 1, "{override_warranties:?}");
    let seven = &override_warranties[0];
    assert_eq!(seven["coverage"]["unit"], "days");
    assert_eq!(
        days_between(&text(&seven["startsOn"]), &text(&seven["endsOn"])),
        6
    );
    assert_eq!(seven["state"], "expiring_soon");

    // The counter lookup works from whatever is scanned or typed.
    let by_serial = lookup(app, &staff, &serial_a).await;
    assert_eq!(by_serial["matchedBy"], "serial");
    assert_eq!(by_serial["groups"].as_array().map(Vec::len), Some(1));
    assert_eq!(by_serial["groups"][0]["serialNumber"], serial_a.as_str());
    let by_prefix = lookup(app, &staff, &serial_a[..9]).await;
    assert_eq!(by_prefix["matchedBy"], "serial");
    let by_invoice = lookup(app, &staff, &invoice_number.to_lowercase()).await;
    assert_eq!(by_invoice["matchedBy"], "invoice_number");
    assert_eq!(by_invoice["groups"].as_array().map(Vec::len), Some(3));
    let by_phone = lookup(app, &staff, "+94 77 123 4567").await;
    assert_eq!(by_phone["matchedBy"], "phone");
    assert!(by_phone["groups"].as_array().map_or(0, Vec::len) >= 4);
    let by_number = lookup(
        app,
        &staff,
        &text(&phone_a["warrantyNumber"]).to_lowercase(),
    )
    .await;
    assert_eq!(by_number["matchedBy"], "warranty_number");
    assert_eq!(
        by_number["groups"][0]["warranties"][0]["key"],
        phone_a["key"]
    );
    let nothing = lookup(app, &staff, "no-such-thing-anywhere").await;
    assert_eq!(nothing["matchedBy"], "nothing");
    assert_eq!(nothing["groups"], json!([]));
    let (status, _) = send(app, "GET", "/api/warranties/lookup?q=%20", None, &staff).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // The detail carries the log and the invoice.
    let phone_a_key = text(&phone_a["key"]);
    let (status, detail) = send(
        app,
        "GET",
        &format!("/api/warranties/{phone_a_key}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["data"]["events"][0]["kind"], "created");
    assert_eq!(detail["data"]["events"][0]["actorName"], "Cashier Kamal");
    assert_eq!(detail["data"]["invoice"]["status"], "paid");
    assert_eq!(detail["data"]["invoice"]["invoiceNumber"], invoice_number);
    let (status, _) = send(app, "GET", "/api/warranties/wty_missing", None, &staff).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Extending needs a manager or owner PIN and a later end.
    let charger_key = text(&charger_w["key"]);
    let extend = |coverage: Value, pin: &str| json!({ "coverage": coverage, "approval": { "managerPin": pin, "reason": "Loyal customer" } });
    let (status, body) = send(
        app,
        "POST",
        &format!("/api/warranties/{charger_key}/extend"),
        Some(extend(
            json!({ "kind": "limited", "value": 3, "unit": "months" }),
            MANAGER_PIN,
        )),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "WARRANTY_NOT_EXTENDED");
    let (status, body) = send(
        app,
        "POST",
        &format!("/api/warranties/{charger_key}/extend"),
        Some(extend(
            json!({ "kind": "limited", "value": 1, "unit": "years" }),
            STAFF_PIN,
        )),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["code"], "INVALID_PIN");
    let (status, extended) = send(
        app,
        "POST",
        &format!("/api/warranties/{charger_key}/extend"),
        Some(extend(
            json!({ "kind": "limited", "value": 1, "unit": "years" }),
            MANAGER_PIN,
        )),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{extended}");
    assert!(text(&extended["data"]["warranty"]["endsOn"]) > text(&charger_w["endsOn"]));
    let last = extended["data"]["events"]
        .as_array()
        .and_then(|e| e.last())
        .expect("the extension is logged")
        .clone();
    assert_eq!(last["kind"], "extended");
    assert_eq!(last["approvedByName"], "mgr");

    // Voiding needs a reason and a manager or owner PIN, once.
    let void_body =
        |pin: &str, reason: &str| json!({ "approval": { "managerPin": pin, "reason": reason } });
    let phone_b = issued
        .iter()
        .find(|w| w["serialNumber"] == serial_b.as_str())
        .expect("phone B has a warranty")
        .clone();
    let phone_b_key = text(&phone_b["key"]);
    let void_uri = format!("/api/warranties/{phone_a_key}/void");
    let (status, _) = send(
        app,
        "POST",
        &void_uri,
        Some(void_body(MANAGER_PIN, "  ")),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(
        app,
        "POST",
        &void_uri,
        Some(void_body("0001", "Seal broken")),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, voided) = send(
        app,
        "POST",
        &void_uri,
        Some(void_body(MANAGER_PIN, "Seal broken")),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{voided}");
    assert_eq!(voided["data"]["warranty"]["state"], "void");
    assert_eq!(voided["data"]["warranty"]["voidReason"], "Seal broken");
    let (status, body) = send(
        app,
        "POST",
        &void_uri,
        Some(void_body(MANAGER_PIN, "Again")),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "WARRANTY_ALREADY_VOID");

    // Returning phone B cancels its warranty; the credit note still knew it was covered.
    let (status, credit) = send(
        app,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [
                { "productKey": phone, "quantity": 1, "reason": "defective",
                  "condition": "resalable", "serialNumber": serial_b },
                { "productKey": charger, "quantity": 3, "reason": "defective",
                  "condition": "resalable" },
            ],
            "paymentMethod": "cash",
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{credit}");
    assert_eq!(credit["data"]["returnedItems"][0]["withinWarranty"], true);
    let after_return = warranties_of(app, &staff, &invoice_key).await;
    for key in [&phone_b_key, &charger_key] {
        let warranty = after_return
            .iter()
            .find(|w| w["key"] == key.as_str())
            .expect("the warranty is listed");
        assert_eq!(warranty["state"], "void", "{warranty}");
        assert!(
            text(&warranty["voidReason"]).starts_with("Returned on "),
            "{warranty}"
        );
    }

    // Voiding the till-override sale cancels its warranty.
    let (status, body) = send(
        app,
        "POST",
        &format!("/api/billing/invoices/{override_invoice}/void"),
        Some(json!({ "reason": "Rang up twice" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Invoice voided successfully", "{body}");
    let after_void = warranties_of(app, &staff, &override_invoice).await;
    assert_eq!(after_void[0]["state"], "void");
    assert_eq!(
        after_void[0]["voidReason"],
        format!(
            "Invoice {} was voided",
            text(&overridden["data"]["invoice"]["invoiceNumber"])
        )
    );

    // A customer's warranties, and the permissions on the reads.
    let (status, body) = send(app, "GET", "/api/warranties", None, &staff).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["total"].as_u64().unwrap_or(0) >= 4);
    let no_permissions = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, _) = send(app, "GET", "/api/warranties", None, &no_permissions).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn warranties_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn warranties_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}

const OLD_ITEMS: &str = "[{\"productKey\":\"prod_old\",\"name\":\"Old Phone\",\"unitPriceCents\":80000,\
    \"quantity\":1,\"discountCents\":0,\"totalCents\":80000,\"unitCostCents\":60000,\
    \"sourceType\":\"retail\",\"returnedQuantity\":0,\"serialNumbers\":[\"SN-OLD-1\"],\
    \"batchAllocations\":[]}]";

#[tokio::test]
async fn an_old_sqlite_database_moves_to_warranty_records() {
    use simplebash_pos_backend::clients::sqlite;
    use simplebash_pos_backend::core::logging::SqlLogging;
    use sqlx::Executor;

    let path = std::env::temp_dir().join(format!("pos_wty_mig_{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("the database opens");
    // Recreate the old shape on top of a fresh schema.
    for statement in [
        "ALTER TABLE products ADD COLUMN warranty_months INTEGER",
        "ALTER TABLE product_serials ADD COLUMN warranty_months INTEGER",
        "ALTER TABLE product_serials ADD COLUMN warranty_expires_at TEXT",
        "INSERT INTO products (key, id, sku, name, category_key, subcategory_key, warranty, \
         warranty_months, version, created_at, updated_at) VALUES \
         ('prod_old', '65f1a1a1a1a1a1a1a1a1a1d1', 'OLD-1', 'Old Phone', 'cat_1', 'subcat_1', \
         '{\"coverage\":{\"kind\":\"none\"}}', 12, 1, '2026-01-01T00:00:00+00:00', \
         '2026-01-01T00:00:00+00:00')",
        "INSERT INTO products (key, id, sku, name, category_key, subcategory_key, warranty, \
         version, created_at, updated_at) VALUES \
         ('prod_none', '65f1a1a1a1a1a1a1a1a1a1d2', 'OLD-2', 'Old Cable', 'cat_1', 'subcat_1', \
         '{\"coverage\":{\"kind\":\"none\"}}', 1, '2026-01-01T00:00:00+00:00', \
         '2026-01-01T00:00:00+00:00')",
        "INSERT INTO product_serials (key, id, product_key, serial_number, batch_key, status, \
         invoice_key, sold_at, warranty_months, warranty_expires_at, version, created_at, \
         updated_at) VALUES ('psn_old', '65f1a1a1a1a1a1a1a1a1a1d3', 'prod_old', 'SN-OLD-1', \
         'stb_1', 'sold', 'inv_old', '2026-03-10T05:00:00+00:00', 12, \
         '2027-03-05T05:00:00+00:00', 1, '2026-01-01T00:00:00+00:00', \
         '2026-03-10T05:00:00+00:00')",
    ] {
        pool.execute(statement)
            .await
            .expect("the old shape is set up");
    }
    sqlx::query(
        "INSERT INTO invoices (key, id, invoice_number, cashier_id, cashier_name_snapshot, \
         customer_phone_snapshot, items, subtotal_cents, total_cents, payment_method, status, \
         shop_profile_snapshot, created_at, updated_at) VALUES ('inv_old', \
         '65f1a1a1a1a1a1a1a1a1a1d4', 'INV-OLD-1', 'usr_1', 'Cashier', '0771234567', ?, 80000, \
         80000, 'cash', 'paid', '{}', '2026-03-10T05:00:00+00:00', '2026-03-10T05:00:00+00:00')",
    )
    .bind(OLD_ITEMS)
    .execute(&pool)
    .await
    .expect("the old invoice is stored");

    sqlite::init_db(&pool).await.expect("the migration runs");

    let profiles: Vec<(String, String)> =
        sqlx::query_as("SELECT key, warranty FROM products ORDER BY key")
            .fetch_all(&pool)
            .await
            .expect("products read");
    let profile = |key: &str| -> Value {
        let (_, text) = profiles
            .iter()
            .find(|(k, _)| k == key)
            .expect("the product exists");
        serde_json::from_str(text).expect("the profile is JSON")
    };
    assert_eq!(profile("prod_old")["coverage"]["value"], 12);
    assert_eq!(profile("prod_old")["warrantyType"], "shop");
    assert_eq!(profile("prod_none")["coverage"]["kind"], "none");
    for (table, column) in [
        ("products", "warranty_months"),
        ("product_serials", "warranty_months"),
        ("product_serials", "warranty_expires_at"),
    ] {
        let columns: Vec<String> =
            sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .fetch_all(&pool)
                .await
                .expect("columns read");
        assert!(
            !columns.iter().any(|c| c == column),
            "{table}.{column} is dropped"
        );
    }
    let migrated: (String, String, String, Option<String>, String, String) = sqlx::query_as(
        "SELECT warranty_number, starts_on, status, ends_on, customer_phone_tail, invoice_number \
         FROM warranties WHERE key = 'wty_mig_psn_old'",
    )
    .fetch_one(&pool)
    .await
    .expect("the sold serial has a warranty");
    assert_eq!(
        migrated,
        (
            "WTY-M-SN-OLD-1".to_string(),
            "2026-03-10".to_string(),
            "active".to_string(),
            Some("2027-03-09".to_string()),
            "771234567".to_string(),
            "INV-OLD-1".to_string(),
        )
    );
    let events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM warranty_events WHERE warranty_key = 'wty_mig_psn_old'",
    )
    .fetch_one(&pool)
    .await
    .expect("events read");
    assert_eq!(events, 1);

    // A second start changes nothing.
    sqlite::init_db(&pool).await.expect("the second start runs");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM warranties")
        .fetch_one(&pool)
        .await
        .expect("warranties read");
    assert_eq!(count, 1);
    drop(pool);
    std::fs::remove_file(&path).ok();
}

#[tokio::test]
async fn an_old_mongo_database_moves_to_warranty_records() {
    use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
    use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
    use simplebash_pos_backend::modules::warranties::migration::migrate_mongo;

    let app = common::spawn_app_isolated().await;
    let db = app.mongo();
    let at = BsonDateTime::parse_rfc3339_str("2026-03-10T05:00:00Z").expect("a date");
    db.collection::<Document>("products")
        .insert_many([
            doc! { "key": "prod_old", "name": "Old Phone", "warranty_months": 12_i64 },
            doc! { "key": "prod_none", "name": "Old Cable" },
        ])
        .await
        .expect("old products stored");
    db.collection::<Document>("product_serials")
        .insert_one(doc! {
            "key": "psn_old", "serial_number": "SN-OLD-1", "product_key": "prod_old",
            "status": "sold", "invoice_key": "inv_old", "sold_at": at,
            "warranty_months": 12_i64, "warranty_expires_at": at,
        })
        .await
        .expect("old serial stored");
    let items: Vec<Document> = serde_json::from_str::<Vec<Value>>(OLD_ITEMS)
        .expect("items parse")
        .into_iter()
        .map(|item| mongodb::bson::serialize_to_document(&item).expect("item converts"))
        .collect();
    db.collection::<Document>("invoices")
        .insert_one(doc! {
            "key": "inv_old", "invoice_number": "INV-OLD-1", "status": "voided",
            "customer_phone_snapshot": "0771234567", "items": items,
        })
        .await
        .expect("old invoice stored");

    let dry = migrate_mongo(db, Mode::DryRun)
        .await
        .expect("the dry run runs");
    assert_eq!(
        (dry.products, dry.products_with_warranty, dry.warranties),
        (2, 1, 1)
    );
    assert_eq!(
        db.collection::<Document>("warranties")
            .count_documents(doc! {})
            .await
            .expect("count"),
        0,
        "a dry run writes nothing"
    );

    migrate_mongo(db, Mode::Apply)
        .await
        .expect("the migration applies");
    let old = db
        .collection::<Document>("products")
        .find_one(doc! { "key": "prod_old" })
        .await
        .expect("read")
        .expect("the product exists");
    assert!(old.get("warranty_months").is_none());
    let profile = old.get_document("warranty").expect("a profile");
    assert_eq!(
        profile
            .get_document("coverage")
            .expect("coverage")
            .get_i64("value")
            .expect("value"),
        12
    );
    let warranty = db
        .collection::<Document>("warranties")
        .find_one(doc! { "key": "wty_mig_psn_old" })
        .await
        .expect("read")
        .expect("the warranty exists");
    assert_eq!(warranty.get_str("status").expect("status"), "void");
    assert_eq!(
        warranty.get_str("void_reason").expect("reason"),
        "Invoice INV-OLD-1 was voided"
    );
    let serial = db
        .collection::<Document>("product_serials")
        .find_one(doc! { "key": "psn_old" })
        .await
        .expect("read")
        .expect("the serial exists");
    assert!(serial.get("warranty_expires_at").is_none());

    let again = migrate_mongo(db, Mode::Apply)
        .await
        .expect("the rerun runs");
    assert_eq!(
        (again.products, again.warranties),
        (0, 0),
        "nothing left to move"
    );
    app.mongo().drop().await.ok();
}
