//! Warranty claims end to end, on SQLite and on a throwaway Atlas database: opening (and the
//! manager override for a warranty that ran out), every step in order and out of order, the
//! supplier trip, the four outcomes (repair, replace a serial or a unit from a price tier,
//! refund through a credit note, reject and a paid repair job), the queue filters, and the
//! warranty's activity log and replacement chain.

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
        pwv: None,
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

async fn detail(app: &common::TestApp, token: &str, key: &str) -> Value {
    let (status, body) = send(app, "GET", &format!("/api/warranties/{key}"), None, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

fn approval(pin: &str, reason: &str) -> Value {
    json!({ "managerPin": pin, "reason": reason })
}

fn checks() -> Value {
    json!({ "screenCracked": false, "liquidDamage": false, "sealBroken": false, "tampered": false })
}

/// Opens a claim on `warranty_key` and returns it.
async fn open(app: &common::TestApp, token: &str, warranty_key: &str) -> Value {
    let (status, body) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(json!({
            "warrantyKey": warranty_key,
            "issueDescription": "  Does not charge  ",
            "voidChecks": checks(),
        })),
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

/// Takes one step on a claim and returns the answer.
async fn claim_step(
    app: &common::TestApp,
    token: &str,
    key: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    send(
        app,
        "POST",
        &format!("/api/warranty-claims/{key}/{path}"),
        body,
        token,
    )
    .await
}

async fn ok_step(app: &common::TestApp, token: &str, key: &str, path: &str, body: Value) -> Value {
    let (status, answer) = claim_step(app, token, key, path, Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{path}: {answer}");
    answer["data"].clone()
}

async fn inspect_and_decide(
    app: &common::TestApp,
    token: &str,
    key: &str,
    decision: &str,
) -> Value {
    ok_step(
        app,
        token,
        key,
        "inspect",
        json!({ "inspectionNotes": "Port is worn" }),
    )
    .await;
    ok_step(
        app,
        token,
        key,
        "decide",
        json!({ "decision": decision, "approval": approval(MANAGER_PIN, "Covered fault") }),
    )
    .await
}

async fn test_warranty_conditions_lifecycle(
    app: &common::TestApp,
    staff: &str,
    warranty_key: &str,
) {
    // 1. Initial list returns default fallback conditions when database is empty
    let (status, body) = send(app, "GET", "/api/warranty-claims/conditions", None, staff).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let conditions = body["data"]["conditions"]
        .as_array()
        .expect("conditions array");
    assert_eq!(conditions.len(), 4);
    assert_eq!(conditions[0]["name"], "Physical damage / cracks");

    // 2. Add custom condition
    let (status, body) = send(
        app,
        "POST",
        "/api/warranty-claims/conditions",
        Some(json!({ "name": "Lens scratched" })),
        staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["name"], "Lens scratched");
    let lens_key = text(&body["data"]["key"]);

    // 3. Open a claim with custom conditions
    let (status, body) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(json!({
            "warrantyKey": warranty_key,
            "issueDescription": "Lens blurry",
            "conditions": ["Lens scratched", "Loose USB port"],
        })),
        staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let claim_key = text(&body["data"]["key"]);
    let claim_conditions = body["data"]["conditions"]
        .as_array()
        .expect("conditions array");
    assert_eq!(claim_conditions.len(), 2);
    assert_eq!(claim_conditions[0], "Lens scratched");
    assert_eq!(claim_conditions[1], "Loose USB port");

    // Resolve and return the test claim so the queue stays clean for subsequent tests
    inspect_and_decide(app, staff, &claim_key, "reject").await;
    ok_step(app, staff, &claim_key, "return", Value::Null).await;

    // 4. Verify shop conditions now include "Loose USB port" because it auto-upserted
    let (status, body) = send(app, "GET", "/api/warranty-claims/conditions", None, staff).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let updated_conditions = body["data"]["conditions"]
        .as_array()
        .expect("conditions array");
    assert!(
        updated_conditions
            .iter()
            .any(|c| c["name"] == "Lens scratched")
    );
    assert!(
        updated_conditions
            .iter()
            .any(|c| c["name"] == "Loose USB port")
    );

    // 5. Delete / deactivate a condition
    let (status, body) = send(
        app,
        "DELETE",
        &format!("/api/warranty-claims/conditions/{lens_key}"),
        None,
        staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // 6. Verify deactivated condition is no longer in active list
    let (status, body) = send(app, "GET", "/api/warranty-claims/conditions", None, staff).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let after_delete = body["data"]["conditions"]
        .as_array()
        .expect("conditions array");
    assert!(!after_delete.iter().any(|c| c["key"] == lens_key));
}

/// Moves a warranty's end date to yesterday, straight in the store.
async fn run_out(app: &common::TestApp, key: &str) {
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(3))
        .date_naive()
        .to_string();
    match &app.db_handle {
        simplebash_pos_backend::clients::db::Db::Sqlite(pool) => {
            sqlx::query("UPDATE warranties SET ends_on = ? WHERE key = ?")
                .bind(&yesterday)
                .bind(key)
                .execute(pool)
                .await
                .expect("the end date moves");
        }
        simplebash_pos_backend::clients::db::Db::Mongo(_) => {
            app.mongo()
                .collection::<mongodb::bson::Document>("warranties")
                .update_one(
                    mongodb::bson::doc! { "key": key },
                    mongodb::bson::doc! { "$set": { "ends_on": &yesterday } },
                )
                .await
                .expect("the end date moves");
        }
    }
}

async fn scenario(app: &common::TestApp) {
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the admin is seeded");
    let admin = token_for(app, "admin", Role::Admin, "Owner Olivia");
    user_with_pin(app, &admin, "mgr", (Role::Manager, "manager"), MANAGER_PIN).await;
    let staff = user_with_pin(app, &admin, "kamal", (Role::Staff, "staff"), STAFF_PIN).await;

    // A serialized phone (12 months) and a charger sold by price tier (6 months).
    let (status, category) = send(
        app,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Phones {}", uuid::Uuid::new_v4()), "icon": "Box", "color": "blue",
            "subcategories": ["Smartphones"],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{category}");
    let base = |name: &str| {
        json!({
            "name": format!("{name} {}", &uuid::Uuid::new_v4().simple().to_string()[..6]),
            "categoryKey": text(&category["data"]["key"]),
            "subcategoryKey": text(&category["data"]["subcategories"][0]["key"]),
            "minStockThreshold": 1,
        })
    };
    let mut phone_body = base("Phone");
    phone_body["isSerialized"] = json!(true);
    phone_body["warranty"] = json!({
        "coverage": { "kind": "limited", "value": 12, "unit": "months" },
        "warrantyType": "shop",
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
    let supplier_key = text(&supplier["data"]["key"]);
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let serials: Vec<String> = (0..4).map(|i| format!("IMEI{}{i}", &tag[..10])).collect();
    let (status, body) = send(
        app,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": supplier_key,
            "date": chrono::Utc::now().to_rfc3339(),
            "lines": [{
                "productKey": phone, "quantity": 4, "lineTotalCents": 200_000,
                "sellingPriceCents": 90_000, "serialNumbers": serials,
            }],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, sold) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(sale(json!([
            { "productKey": phone, "quantity": 2, "discountCents": 0, "sourceType": "retail",
              "serialNumbers": [serials[0], serials[1]] },
            { "productKey": charger, "quantity": 2, "discountCents": 0, "sourceType": "retail",
              "sellingPriceCents": 1500 },
        ]))),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    let invoice_key = text(&sold["data"]["invoice"]["key"]);
    let issued = warranties_of(app, &staff, &invoice_key).await;
    let warranty_of = |serial: Option<&str>| {
        issued
            .iter()
            .find(|w| w["serialNumber"].as_str() == serial)
            .expect("the unit has a warranty")
            .clone()
    };
    let phone_a = warranty_of(Some(&serials[0]));
    let phone_b = warranty_of(Some(&serials[1]));
    let charger_w = warranty_of(None);
    let phone_a_key = text(&phone_a["key"]);

    // ---- Verify dynamic warranty conditions lifecycle ----
    let (status, cond_sale) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(sale(json!([
            { "productKey": charger, "quantity": 1, "discountCents": 0, "sourceType": "retail",
              "sellingPriceCents": 1500 },
        ]))),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cond_sale}");
    let cond_inv = text(&cond_sale["data"]["invoice"]["key"]);
    let cond_warranties = warranties_of(app, &staff, &cond_inv).await;
    let cond_wty_key = text(&cond_warranties[0]["key"]);
    test_warranty_conditions_lifecycle(app, &staff, &cond_wty_key).await;

    // ---- Phone A: to the supplier and back, then replaced from stock ----
    let claim = open(app, &staff, &phone_a_key).await;
    let claim_key = text(&claim["key"]);
    assert!(text(&claim["claimNumber"]).starts_with("WCL-"), "{claim}");
    assert_eq!(claim["status"], "received");
    assert_eq!(claim["issueDescription"], "Does not charge");
    assert_eq!(claim["serialNumber"], serials[0].as_str());
    assert_eq!(claim["ageDays"], 0);
    let held = detail(app, &staff, &phone_a_key).await;
    assert_eq!(held["warranty"]["state"], "under_claim");
    assert_eq!(held["warranty"]["claimCount"], 1);
    assert_eq!(held["claims"][0]["key"], claim_key.as_str());

    // One open claim per item; steps only in order.
    let (status, body) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(json!({ "warrantyKey": phone_a_key, "issueDescription": "Again", "voidChecks": checks() })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "CLAIM_ALREADY_OPEN");
    let (status, body) = claim_step(
        app,
        &staff,
        &claim_key,
        "decide",
        Some(json!({ "decision": "repair", "approval": approval(MANAGER_PIN, "Fine") })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "CLAIM_STEP_NOT_ALLOWED");
    let (status, body) = claim_step(
        app,
        &staff,
        &claim_key,
        "inspect",
        Some(json!({ "inspectionNotes": "   " })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let inspected = ok_step(
        app,
        &staff,
        &claim_key,
        "inspect",
        json!({ "inspectionNotes": "Board fault" }),
    )
    .await;
    assert_eq!(inspected["status"], "inspected");

    let (status, body) = claim_step(
        app,
        &staff,
        &claim_key,
        "supplier/send",
        Some(json!({ "supplierKey": supplier_key, "expectedBackOn": "2020-01-01" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let sent = ok_step(
        app,
        &staff,
        &claim_key,
        "supplier/send",
        json!({ "supplierKey": supplier_key, "reference": " RMA-77 " }),
    )
    .await;
    assert_eq!(sent["status"], "at_supplier");
    assert_eq!(sent["supplier"]["reference"], "RMA-77");
    assert_eq!(sent["supplier"]["supplierKey"], supplier_key.as_str());
    assert_eq!(sent["supplierOverdue"], false);
    let (status, queue) = send(
        app,
        "GET",
        &format!("/api/warranty-claims?status=at_supplier&supplierKey={supplier_key}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{queue}");
    assert_eq!(items(&queue).len(), 1, "{queue}");
    let (status, body) = claim_step(
        app,
        &staff,
        &claim_key,
        "decide",
        Some(json!({ "decision": "replace", "approval": approval(MANAGER_PIN, "Fine") })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let back = ok_step(
        app,
        &staff,
        &claim_key,
        "supplier/return",
        json!({ "outcome": "rejected", "note": "Out of their cover" }),
    )
    .await;
    assert_eq!(back["status"], "inspected");
    assert_eq!(back["supplier"]["outcome"], "rejected");

    // Only a manager's or the owner's PIN decides.
    let (status, body) = claim_step(
        app,
        &staff,
        &claim_key,
        "decide",
        Some(json!({ "decision": "replace", "approval": approval(STAFF_PIN, "Me") })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    let decided = ok_step(
        app,
        &staff,
        &claim_key,
        "decide",
        json!({ "decision": "replace", "approval": approval(MANAGER_PIN, "Dead board") }),
    )
    .await;
    assert_eq!(decided["status"], "decided");
    assert_eq!(decided["decision"], "replace");
    assert_eq!(decided["decidedByName"], "mgr");

    for (replacement, why) in [
        (Value::Null, "no unit"),
        (json!({ "serialNumber": serials[0] }), "the faulty unit"),
        (json!({ "serialNumber": serials[1] }), "a unit already sold"),
        (
            json!({ "sellingPriceCents": 90_000 }),
            "a price for a serialized item",
        ),
    ] {
        let (status, body) = claim_step(
            app,
            &staff,
            &claim_key,
            "resolve",
            Some(json!({ "replacement": replacement })),
        )
        .await;
        assert!(status.is_client_error(), "{why}: {body}");
    }
    let resolved = ok_step(
        app,
        &staff,
        &claim_key,
        "resolve",
        json!({ "replacement": { "serialNumber": serials[2] }, "resolutionNote": "Swapped" }),
    )
    .await;
    assert_eq!(resolved["status"], "resolved");
    assert_eq!(resolved["replacementSerialNumber"], serials[2].as_str());
    let new_key = text(&resolved["replacementWarrantyKey"]);
    let old = detail(app, &staff, &phone_a_key).await;
    assert_eq!(old["warranty"]["state"], "void");
    assert_eq!(old["warranty"]["replacedByWarrantyKey"], new_key.as_str());
    let new = detail(app, &staff, &new_key).await;
    assert_eq!(new["warranty"]["serialNumber"], serials[2].as_str());
    assert_eq!(new["warranty"]["endsOn"], phone_a["endsOn"]);
    assert_eq!(new["warranty"]["invoiceKey"], invoice_key.as_str());
    assert_eq!(new["replaces"]["key"], phone_a_key.as_str());
    let kinds: Vec<&str> = old["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|e| e["kind"].as_str().expect("a kind"))
        .collect();
    for kind in [
        "claim_opened",
        "supplier_sent",
        "supplier_returned",
        "claim_decided",
        "replaced",
    ] {
        assert!(kinds.contains(&kind), "{kind} in {kinds:?}");
    }
    let found = lookup(app, &staff, &serials[2]).await;
    assert_eq!(found["groups"][0]["warranties"][0]["key"], new_key.as_str());
    let (status, movements) = send(
        app,
        "GET",
        "/api/inventory/stock-movements?movementType=warranty_replacement",
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{movements}");
    assert!(movements.to_string().contains(&claim_key), "{movements}");

    let returned = ok_step(app, &staff, &claim_key, "return", Value::Null).await;
    assert_eq!(returned["status"], "returned");
    let (status, body) = claim_step(app, &staff, &claim_key, "return", None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // ---- The charger: repaired, then one of the two units replaced by price ----
    let charger_key = text(&charger_w["key"]);
    // Multi-unit partial claim: 1 of 2 units claimed
    let (status, excessive) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(json!({
            "warrantyKey": charger_key,
            "quantity": 3,
            "issueDescription": "Too many units",
            "voidChecks": { "physicalDamage": true, "liquidDamage": false, "sealBroken": false, "tampered": false },
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{excessive}");

    let repair = open(app, &staff, &charger_key).await;
    let repair_key = text(&repair["key"]);
    assert_eq!(repair["quantity"], 1);
    let during_partial = detail(app, &staff, &charger_key).await;
    assert_eq!(during_partial["warranty"]["claimedQuantity"], 1);
    assert_eq!(during_partial["warranty"]["quantity"], 2);
    assert_eq!(
        during_partial["warranty"]["state"], "active",
        "partial claim leaves remaining units active"
    );

    inspect_and_decide(app, &staff, &repair_key, "repair").await;
    let (status, body) = claim_step(
        app,
        &staff,
        &repair_key,
        "resolve",
        Some(json!({ "replacement": { "sellingPriceCents": 1500 } })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    ok_step(app, &staff, &repair_key, "resolve", json!({})).await;
    let repaired = detail(app, &staff, &charger_key).await;
    assert_eq!(repaired["warranty"]["state"], "active");
    assert_eq!(repaired["warranty"]["endsOn"], charger_w["endsOn"]);
    ok_step(app, &staff, &repair_key, "return", Value::Null).await;
    let returned_detail = detail(app, &staff, &charger_key).await;
    assert_eq!(returned_detail["warranty"]["claimedQuantity"], 0);

    let swap = open(app, &staff, &charger_key).await;
    let swap_key = text(&swap["key"]);
    inspect_and_decide(app, &staff, &swap_key, "replace").await;
    let swapped = ok_step(
        app,
        &staff,
        &swap_key,
        "resolve",
        json!({ "replacement": { "sellingPriceCents": 1500 } }),
    )
    .await;
    let one = detail(app, &staff, &text(&swapped["replacementWarrantyKey"])).await;
    assert_eq!(one["warranty"]["quantity"], 1);
    assert!(one["warranty"]["serialNumber"].is_null());
    let rest = detail(app, &staff, &charger_key).await;
    assert_eq!(
        rest["warranty"]["state"], "active",
        "the other unit stays covered"
    );
    assert_eq!(rest["warranty"]["quantity"], 1);
    assert_eq!(rest["warranty"]["claimCount"], 2);

    // ---- Phone B: refunded through a credit note ----
    let phone_b_key = text(&phone_b["key"]);
    let refund = open(app, &staff, &phone_b_key).await;
    let refund_key = text(&refund["key"]);
    inspect_and_decide(app, &staff, &refund_key, "refund").await;
    let (status, body) = claim_step(app, &staff, &refund_key, "resolve", Some(json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let credit = |serial: &str| {
        json!({
            "invoiceKey": invoice_key,
            "warrantyClaimKey": refund_key,
            "returnedItems": [{ "productKey": phone, "quantity": 1, "reason": "defective",
                "condition": "damaged", "disposition": "repair_pending", "serialNumber": serial }],
            "paymentMethod": "cash",
        })
    };
    let (status, body) = send(
        app,
        "POST",
        "/api/billing/credit-notes",
        Some(credit(&serials[2])),
        &staff,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "the claimed unit must come back: {body}"
    );
    let (status, body) = send(
        app,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "warrantyClaimKey": refund_key,
            "returnedItems": [{ "productKey": phone, "quantity": 2, "reason": "defective",
                "condition": "damaged", "disposition": "repair_pending", "serialNumber": serials[1] }],
            "paymentMethod": "cash",
        })),
        &staff,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "serialized qty must be 1: {body}"
    );
    let (status, note) = send(
        app,
        "POST",
        "/api/billing/credit-notes",
        Some(credit(&serials[1])),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{note}");
    let (status, refunded) = send(
        app,
        "GET",
        &format!("/api/warranty-claims/{refund_key}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{refunded}");
    assert_eq!(refunded["data"]["status"], "resolved");
    assert_eq!(refunded["data"]["creditNoteKey"], note["data"]["key"]);
    assert_eq!(
        detail(app, &staff, &phone_b_key).await["warranty"]["state"],
        "void"
    );
    let (status, body) = claim_step(app, &staff, &refund_key, "return", None).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a refunded item stays: {body}"
    );

    // ---- Multi-unit non-serialized claim: exact refund quantity enforcement ----
    let mut bulk_body = base("Bulk Item");
    bulk_body["openingStock"] =
        json!({ "quantity": 10, "unitCostCents": 500, "sellingPriceCents": 1500 });
    bulk_body["warranty"] = json!({
        "coverage": { "kind": "limited", "value": 6, "unit": "months" },
        "warrantyType": "shop",
    });
    let bulk_item = product(app, &admin, bulk_body).await;

    let (status, multi_sale) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(sale(json!([
            { "productKey": bulk_item, "quantity": 4, "discountCents": 0, "sourceType": "retail",
              "sellingPriceCents": 1500 },
        ]))),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{multi_sale}");
    let multi_inv_key = text(&multi_sale["data"]["invoice"]["key"]);
    let multi_warranties = warranties_of(app, &staff, &multi_inv_key).await;
    let multi_wty_key = text(&multi_warranties[0]["key"]);
    assert_eq!(multi_warranties[0]["quantity"], 4);

    let (status, claim_3) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(json!({
            "warrantyKey": multi_wty_key,
            "quantity": 3,
            "issueDescription": "3 units defective",
            "voidChecks": { "physicalDamage": false, "liquidDamage": false, "sealBroken": false, "tampered": false },
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claim_3}");
    let claim_3_key = text(&claim_3["data"]["key"]);
    inspect_and_decide(app, &staff, &claim_3_key, "refund").await;

    // Attempt refund with quantity 1 (lower than claimed 3) -> rejected
    let (status, body) = send(
        app,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": multi_inv_key,
            "warrantyClaimKey": claim_3_key,
            "returnedItems": [{ "productKey": bulk_item, "quantity": 1, "reason": "defective",
                "condition": "damaged", "disposition": "repair_pending" }],
            "paymentMethod": "cash",
        })),
        &staff,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "lower quantity rejected: {body}"
    );

    // Attempt refund with quantity 4 (higher than claimed 3) -> rejected
    let (status, body) = send(
        app,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": multi_inv_key,
            "warrantyClaimKey": claim_3_key,
            "returnedItems": [{ "productKey": bulk_item, "quantity": 4, "reason": "defective",
                "condition": "damaged", "disposition": "repair_pending" }],
            "paymentMethod": "cash",
        })),
        &staff,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "higher quantity rejected: {body}"
    );

    // Attempt refund with exact quantity 3 -> succeeds
    let (status, note_3) = send(
        app,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": multi_inv_key,
            "warrantyClaimKey": claim_3_key,
            "returnedItems": [{ "productKey": bulk_item, "quantity": 3, "reason": "defective",
                "condition": "damaged", "disposition": "repair_pending" }],
            "paymentMethod": "cash",
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{note_3}");
    let after_claim = detail(app, &staff, &multi_wty_key).await;
    assert_eq!(
        after_claim["warranty"]["quantity"], 1,
        "4 - 3 = 1 unit remains"
    );
    assert_eq!(after_claim["warranty"]["claimedQuantity"], 0);
    assert_eq!(after_claim["warranty"]["state"], "active");

    // ---- A warranty that ran out: honoured only with a manager's PIN, then rejected ----
    let new_phone_key = new_key;
    run_out(app, &new_phone_key).await;
    let (status, body) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(json!({ "warrantyKey": new_phone_key, "issueDescription": "Cracked", "voidChecks": checks() })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "WARRANTY_EXPIRED");
    let (status, late) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(json!({
            "warrantyKey": new_phone_key, "issueDescription": "Cracked",
            "voidChecks": { "screenCracked": true, "liquidDamage": false, "sealBroken": false, "tampered": false },
            "approval": approval(MANAGER_PIN, "Loyal customer"),
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{late}");
    assert_eq!(late["data"]["expiredOverrideByName"], "mgr");
    assert_eq!(late["data"]["voidChecks"]["screenCracked"], true);
    let late_key = text(&late["data"]["key"]);
    let rejected = inspect_and_decide(app, &staff, &late_key, "reject").await;
    assert_eq!(
        rejected["status"], "resolved",
        "a rejection ends the claim at once"
    );
    let after = detail(app, &staff, &new_phone_key).await;
    assert_eq!(
        after["warranty"]["state"], "expired",
        "no longer held by the claim"
    );
    let (status, paid) = claim_step(app, &staff, &late_key, "paid-repair", None).await;
    assert_eq!(status, StatusCode::OK, "{paid}");
    assert!(
        text(&paid["data"]["paidRepairNumber"]).starts_with("REP-"),
        "{paid}"
    );
    let (status, body) = claim_step(app, &staff, &late_key, "paid-repair", None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    ok_step(app, &staff, &late_key, "return", Value::Null).await;

    // ---- The queue ----
    let (status, open_now) =
        send(app, "GET", "/api/warranty-claims?status=open", None, &staff).await;
    assert_eq!(status, StatusCode::OK, "{open_now}");
    assert_eq!(open_now["data"]["total"], 0, "{open_now}");
    let (status, all) = send(
        app,
        "GET",
        &format!("/api/warranty-claims?status=all&invoiceKey={invoice_key}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{all}");
    assert_eq!(all["data"]["total"], 5, "{all}");
    let numbers: Vec<String> = items(&all)
        .iter()
        .map(|c| text(&c["claimNumber"]))
        .collect();
    let mut sorted = numbers.clone();
    sorted.sort();
    assert_eq!(numbers, sorted, "oldest first");
    let (status, found) = send(
        app,
        "GET",
        &format!("/api/warranty-claims?status=all&q={}", &serials[1][4..]),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["data"]["total"], 1, "{found}");

    // Permissions.
    let viewer = common::mint_token(&app.config, Some(Role::Staff), &["warranties:view"]);
    let (status, _) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(
            json!({ "warrantyKey": charger_key, "issueDescription": "x", "voidChecks": checks() }),
        ),
        &viewer,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let nobody = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, _) = send(app, "GET", "/api/warranty-claims?status=all", None, &nobody).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // ---- Voiding an invoice after replacements ----
    // A second delivery of phones, so the unit given out comes from another batch.
    let more: Vec<String> = (4..6).map(|i| format!("IMEI{}{i}", &tag[..10])).collect();
    let (status, body) = send(
        app,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": supplier_key,
            "date": chrono::Utc::now().to_rfc3339(),
            "lines": [{
                "productKey": phone, "quantity": 2, "lineTotalCents": 120_000,
                "sellingPriceCents": 90_000, "serialNumbers": more,
            }],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let phone_units = serial_list(app, &admin, &phone).await;
    let batch_of = |serial: &str| text(&unit(&phone_units, serial)["batchKey"]);
    let (old_batch, new_batch) = (batch_of(&serials[3]), batch_of(&more[0]));
    assert_ne!(old_batch, new_batch);
    let phone_before = remaining(app, &admin, &phone).await;
    let charger_before = remaining(app, &admin, &charger).await;

    let (status, sold) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(sale(json!([
            { "productKey": phone, "quantity": 1, "discountCents": 0, "sourceType": "retail",
              "serialNumbers": [serials[3]] },
            { "productKey": charger, "quantity": 1, "discountCents": 0, "sourceType": "retail",
              "sellingPriceCents": 1500 },
        ]))),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    let second_invoice = text(&sold["data"]["invoice"]["key"]);
    let issued = warranties_of(app, &staff, &second_invoice).await;
    let key_of = |serial: Option<&str>| {
        text(
            &issued
                .iter()
                .find(|w| w["serialNumber"].as_str() == serial)
                .expect("the unit has a warranty")["key"],
        )
    };

    // The phone is swapped for a unit of the second delivery.
    let phone_claim = text(&open(app, &staff, &key_of(Some(&serials[3]))).await["key"]);
    inspect_and_decide(app, &staff, &phone_claim, "replace").await;
    ok_step(
        app,
        &staff,
        &phone_claim,
        "resolve",
        json!({ "replacement": { "serialNumber": more[0] } }),
    )
    .await;
    // The charger is swapped twice: the second claim is on the first replacement.
    let mut charger_warranty = key_of(None);
    for _ in 0..2 {
        let claim = text(&open(app, &staff, &charger_warranty).await["key"]);
        inspect_and_decide(app, &staff, &claim, "replace").await;
        let swapped = ok_step(
            app,
            &staff,
            &claim,
            "resolve",
            json!({ "replacement": { "sellingPriceCents": 1500 } }),
        )
        .await;
        charger_warranty = text(&swapped["replacementWarrantyKey"]);
    }

    let (status, voided) = send(
        app,
        "POST",
        &format!("/api/billing/invoices/{second_invoice}/void"),
        Some(json!({ "reason": "Customer changed their mind" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{voided}");
    assert_eq!(voided["message"], "Invoice voided successfully", "{voided}");

    // The unit the customer had goes back to its own batch; the faulty one stays out.
    let phone_units = serial_list(app, &admin, &phone).await;
    assert_eq!(unit(&phone_units, &more[0])["status"], "in_stock");
    assert_eq!(unit(&phone_units, &serials[3])["status"], "returned_faulty");
    let phone_after = remaining(app, &admin, &phone).await;
    assert_eq!(
        phone_after[&old_batch],
        phone_before[&old_batch] - 1,
        "the faulty unit's delivery is one short"
    );
    assert_eq!(phone_after[&new_batch], phone_before[&new_batch]);
    let charger_after = remaining(app, &admin, &charger).await;
    assert_eq!(
        charger_after.values().sum::<i64>(),
        charger_before.values().sum::<i64>() - 2,
        "both faulty chargers stay out; the last one given comes back"
    );
}

/// What each of the product's batches has left, by batch key.
async fn remaining(
    app: &common::TestApp,
    token: &str,
    product_key: &str,
) -> std::collections::HashMap<String, i64> {
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/inventory/products/{product_key}/batches"),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    items(&body)
        .iter()
        .map(|batch| {
            (
                text(&batch["key"]),
                batch["remainingQuantity"].as_i64().expect("a count"),
            )
        })
        .collect()
}

async fn serial_list(app: &common::TestApp, token: &str, product_key: &str) -> Vec<Value> {
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/inventory/products/{product_key}/serials"),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    items(&body)
}

fn unit<'a>(units: &'a [Value], serial: &str) -> &'a Value {
    units
        .iter()
        .find(|unit| unit["serialNumber"] == serial)
        .expect("the unit is tracked")
}

async fn lookup(app: &common::TestApp, token: &str, q: &str) -> Value {
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/warranties/lookup?q={q}"),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

/// The claims a warranty has, any status.
async fn claims_of(app: &common::TestApp, token: &str, warranty_key: &str) -> Vec<Value> {
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/warranty-claims?status=all&warrantyKey={warranty_key}"),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    items(&body)
}

/// `POST /warranty-claims/process` with every field a counter sends.
fn process_body(warranty_key: &str, outcome: Value, approval: Value) -> Value {
    json!({
        "warrantyKey": warranty_key,
        "issueDescription": "  Will not switch on  ",
        "inspectionNotes": "Board is dead, no liquid marks",
        "outcome": outcome,
        "approval": approval,
    })
}

/// A process request whose outcome is a replacement.
fn replace_body(warranty_key: &str, replacement: Value, approval: Value) -> Value {
    process_body(
        warranty_key,
        json!({ "decision": "replace", "replacement": replacement }),
        approval,
    )
}

async fn process(app: &common::TestApp, token: &str, body: Value) -> (StatusCode, Value) {
    send(
        app,
        "POST",
        "/api/warranty-claims/process",
        Some(body),
        token,
    )
    .await
}

/// One request opens, inspects and decides a claim and carries out the decision's outcome.
/// Everything that can be refused is refused before anything is written.
async fn process_claim_scenario(app: &common::TestApp) {
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the admin is seeded");
    let admin = token_for(app, "admin", Role::Admin, "Owner Olivia");
    user_with_pin(app, &admin, "mgr", (Role::Manager, "manager"), MANAGER_PIN).await;
    let staff = user_with_pin(app, &admin, "kamal", (Role::Staff, "staff"), STAFF_PIN).await;

    let (status, category) = send(
        app,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Phones {}", uuid::Uuid::new_v4()), "icon": "Box", "color": "blue",
            "subcategories": ["Smartphones"],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{category}");
    let base = |name: &str| {
        json!({
            "name": format!("{name} {}", &uuid::Uuid::new_v4().simple().to_string()[..6]),
            "categoryKey": text(&category["data"]["key"]),
            "subcategoryKey": text(&category["data"]["subcategories"][0]["key"]),
            "minStockThreshold": 1,
        })
    };
    let mut phone_body = base("Phone");
    phone_body["isSerialized"] = json!(true);
    phone_body["warranty"] = json!({
        "coverage": { "kind": "limited", "value": 12, "unit": "months" },
        "warrantyType": "shop",
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
    let serials: Vec<String> = (0..13).map(|i| format!("QIMEI{}{i}", &tag[..10])).collect();
    let (status, body) = send(
        app,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": text(&supplier["data"]["key"]),
            "date": chrono::Utc::now().to_rfc3339(),
            "lines": [{
                "productKey": phone, "quantity": 13, "lineTotalCents": 650_000,
                "sellingPriceCents": 90_000, "serialNumbers": serials,
            }],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // Ten phones and one charger are sold; three phones stay in stock to be given out.
    let (status, sold) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(sale(json!([
            { "productKey": phone, "quantity": 10, "discountCents": 0, "sourceType": "retail",
              "serialNumbers": [serials[0], serials[1], serials[2], serials[6], serials[7],
                  serials[8], serials[9], serials[10], serials[11], serials[12]] },
            { "productKey": charger, "quantity": 1, "discountCents": 0, "sourceType": "retail",
              "sellingPriceCents": 1500 },
        ]))),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    let invoice_key = text(&sold["data"]["invoice"]["key"]);
    let issued = warranties_of(app, &staff, &invoice_key).await;
    let warranty_of = |serial: Option<&str>| {
        text(
            &issued
                .iter()
                .find(|w| w["serialNumber"].as_str() == serial)
                .expect("the unit has a warranty")["key"],
        )
    };
    let (phone_a, phone_b, phone_c, charger_w) = (
        warranty_of(Some(&serials[0])),
        warranty_of(Some(&serials[1])),
        warranty_of(Some(&serials[2])),
        warranty_of(None),
    );
    let by_serial = |serial: &str| json!({ "serialNumber": serial });

    // ---- Refused before anything is written ----
    // Nothing written about what was found.
    let mut no_notes = replace_body(
        &phone_a,
        by_serial(&serials[4]),
        approval(MANAGER_PIN, "Covered"),
    );
    no_notes["inspectionNotes"] = json!("   ");
    let (status, body) = process(app, &staff, no_notes).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // A cashier's own PIN is not a manager's.
    let (status, body) = process(
        app,
        &staff,
        replace_body(&phone_a, by_serial(&serials[4]), approval(STAFF_PIN, "Me")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["code"], "INVALID_PIN", "{body}");
    // A unit that is not in stock, and the faulty unit itself.
    let (status, body) = process(
        app,
        &staff,
        replace_body(
            &phone_a,
            by_serial(&format!("NOT-IN-STOCK-{tag}")),
            approval(MANAGER_PIN, "Covered"),
        ),
    )
    .await;
    assert!(status.is_client_error(), "{status}: {body}");
    let (status, body) = process(
        app,
        &staff,
        replace_body(
            &phone_a,
            by_serial(&serials[0]),
            approval(MANAGER_PIN, "Covered"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "CLAIM_REPLACEMENT_INVALID", "{body}");
    // Without the permission to record claims.
    let nobody = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, body) = process(
        app,
        &nobody,
        replace_body(
            &phone_a,
            by_serial(&serials[4]),
            approval(MANAGER_PIN, "Covered"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(claims_of(app, &staff, &phone_a).await.is_empty());
    assert_eq!(
        detail(app, &staff, &phone_a).await["warranty"]["state"],
        "active"
    );

    // ---- A serialized phone is swapped in one request ----
    let old_ends_on = detail(app, &staff, &phone_a).await["warranty"]["endsOn"].clone();
    let (status, body) = process(
        app,
        &staff,
        replace_body(
            &phone_a,
            by_serial(&serials[4]),
            approval(MANAGER_PIN, "Dead board"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let claim = &body["data"];
    assert_eq!(claim["status"], "returned", "{body}");
    assert_eq!(claim["decision"], "replace");
    assert_eq!(claim["decidedByName"], "mgr");
    assert_eq!(claim["decisionReason"], "Dead board");
    assert_eq!(claim["issueDescription"], "Will not switch on");
    assert_eq!(claim["inspectionNotes"], "Board is dead, no liquid marks");
    assert_eq!(claim["replacementSerialNumber"], serials[4].as_str());
    assert!(!claim["returnedOn"].is_null(), "{claim}");
    let new_key = text(&claim["replacementWarrantyKey"]);
    let old = detail(app, &staff, &phone_a).await;
    assert_eq!(old["warranty"]["replacedByWarrantyKey"], new_key.as_str());
    let new = detail(app, &staff, &new_key).await;
    assert_eq!(
        new["warranty"]["endsOn"], old_ends_on,
        "the old end date carries on"
    );
    assert_eq!(new["replaces"]["key"], phone_a.as_str());
    let units = serial_list(app, &admin, &phone).await;
    assert_eq!(unit(&units, &serials[0])["status"], "returned_faulty");
    assert_eq!(unit(&units, &serials[4])["status"], "sold");
    // The swapped warranty cannot be swapped again.
    let (status, body) = process(
        app,
        &staff,
        replace_body(
            &phone_a,
            by_serial(&serials[5]),
            approval(MANAGER_PIN, "Again"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(claims_of(app, &staff, &phone_a).await.len(), 1);

    // ---- A warranty that already has an open claim, or has run out ----
    open(app, &staff, &phone_b).await;
    let (status, body) = process(
        app,
        &staff,
        replace_body(
            &phone_b,
            by_serial(&serials[5]),
            approval(MANAGER_PIN, "Covered"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "CLAIM_ALREADY_OPEN", "{body}");
    assert_eq!(claims_of(app, &staff, &phone_b).await.len(), 1);

    run_out(app, &phone_c).await;
    // Staff with no PIN cannot decide, ended warranty or not, and nothing is written.
    let (status, body) = process(
        app,
        &staff,
        replace_body(&phone_c, by_serial(&serials[5]), Value::Null),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PIN_NOT_SET", "{body}");
    assert!(claims_of(app, &staff, &phone_c).await.is_empty());

    // ---- A signed-in owner approves their own claim: no PIN, no reason ----
    // Sending a PIN is refused before anything is written, in the quick replace and the steps.
    let (status, body) = process(
        app,
        &admin,
        replace_body(
            &phone_c,
            by_serial(&serials[5]),
            approval(MANAGER_PIN, "Covered"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(claims_of(app, &staff, &phone_c).await.is_empty());
    // The ended warranty is honoured by the owner without a second person.
    let honoured = open(app, &admin, &phone_c).await;
    assert_eq!(honoured["expiredOverrideByName"], "Owner Olivia");
    let honoured_key = text(&honoured["key"]);
    ok_step(
        app,
        &admin,
        &honoured_key,
        "inspect",
        json!({ "inspectionNotes": "Port is worn" }),
    )
    .await;
    let (status, body) = claim_step(
        app,
        &admin,
        &honoured_key,
        "decide",
        Some(json!({ "decision": "repair", "approval": approval(MANAGER_PIN, "Covered") })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let decided = ok_step(
        app,
        &admin,
        &honoured_key,
        "decide",
        json!({ "decision": "repair" }),
    )
    .await;
    assert_eq!(decided["decidedByName"], "Owner Olivia");
    assert!(decided["decisionReason"].is_null(), "{decided}");
    // Staff still need a manager's PIN for the same step.
    let waiting = text(&claims_of(app, &staff, &phone_b).await[0]["key"]);
    ok_step(
        app,
        &staff,
        &waiting,
        "inspect",
        json!({ "inspectionNotes": "Port is worn" }),
    )
    .await;
    let (status, body) = claim_step(
        app,
        &staff,
        &waiting,
        "decide",
        Some(json!({ "decision": "repair" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PIN_NOT_SET", "{body}");

    // ---- A charger is swapped from a price tier ----
    let charger_before: i64 = remaining(app, &admin, &charger).await.values().sum();
    let (status, body) = process(
        app,
        &staff,
        replace_body(
            &charger_w,
            json!({ "sellingPriceCents": 1500 }),
            approval(MANAGER_PIN, "Faulty plug"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "returned", "{body}");
    let charger_after: i64 = remaining(app, &admin, &charger).await.values().sum();
    assert_eq!(charger_after, charger_before - 1, "one unit left stock");

    // ---- Every outcome, processed by an owner with no PIN and no reason ----
    let (phone_d, phone_e, phone_f, phone_g, phone_h, phone_i, phone_j) = (
        warranty_of(Some(&serials[6])),
        warranty_of(Some(&serials[7])),
        warranty_of(Some(&serials[8])),
        warranty_of(Some(&serials[9])),
        warranty_of(Some(&serials[10])),
        warranty_of(Some(&serials[11])),
        warranty_of(Some(&serials[12])),
    );
    let state_of = |warranty: String| {
        let (app, admin) = (app, admin.clone());
        async move { detail(app, &admin, &warranty).await["warranty"]["state"].clone() }
    };
    // A repair finished now is handed back and the warranty carries on.
    let (status, body) = process(
        app,
        &admin,
        process_body(
            &phone_d,
            json!({ "decision": "repair", "finish": { "when": "now", "resolutionNote": "New port" } }),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "returned", "{body}");
    assert_eq!(body["data"]["decision"], "repair");
    assert_eq!(body["data"]["decidedByName"], "Owner Olivia");
    assert!(body["data"]["decisionReason"].is_null(), "{body}");
    assert_eq!(body["data"]["resolutionNote"], "New port");
    assert_eq!(state_of(phone_d.clone()).await, "active");
    // A repair left for later stops once it is decided, and the warranty stays held.
    let (status, body) = process(
        app,
        &admin,
        process_body(
            &phone_e,
            json!({ "decision": "repair", "finish": { "when": "later" } }),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "decided", "{body}");
    assert_eq!(body["data"]["decision"], "repair");
    assert_eq!(state_of(phone_e.clone()).await, "under_claim");
    // A refund stops once it is decided: the credit note finishes it, and no stock moves.
    let before = serial_list(app, &admin, &phone).await;
    let (status, body) = process(
        app,
        &admin,
        process_body(&phone_f, json!({ "decision": "refund" }), Value::Null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "decided", "{body}");
    assert_eq!(body["data"]["decision"], "refund");
    assert_eq!(state_of(phone_f.clone()).await, "under_claim");
    let after = serial_list(app, &admin, &phone).await;
    assert_eq!(
        unit(&after, &serials[8])["status"],
        unit(&before, &serials[8])["status"]
    );
    // A rejection resolves at once; it is handed back only when asked.
    let (status, body) = process(
        app,
        &admin,
        process_body(
            &phone_g,
            json!({ "decision": "reject", "handBack": true }),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "returned", "{body}");
    assert_eq!(state_of(phone_g.clone()).await, "active");
    let (status, body) = process(
        app,
        &admin,
        process_body(
            &phone_h,
            json!({ "decision": "reject", "handBack": false }),
            Value::Null,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "resolved", "{body}");
    assert!(body["data"]["returnedOn"].is_null(), "{body}");
    // A warranty that ran out is honoured by an owner swapping the unit, with no approval sent.
    run_out(app, &phone_i).await;
    let (status, body) = process(
        app,
        &admin,
        replace_body(&phone_i, by_serial(&serials[3]), Value::Null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "returned", "{body}");
    assert_eq!(body["data"]["expiredOverrideByName"], "Owner Olivia");
    // The same for Staff needs a manager's PIN, which then honours both the ended warranty and
    // the decision.
    run_out(app, &phone_j).await;
    let (status, body) = process(
        app,
        &staff,
        process_body(
            &phone_j,
            json!({ "decision": "repair", "finish": { "when": "later" } }),
            approval(MANAGER_PIN, "Goodwill"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "decided", "{body}");
    assert_eq!(body["data"]["expiredOverrideByName"], "mgr");
    assert_eq!(body["data"]["decidedByName"], "mgr");

    // ---- Voiding the invoice puts the swapped phone back ----
    let (status, voided) = send(
        app,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/void"),
        Some(json!({ "reason": "Customer changed their mind" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{voided}");
    let units = serial_list(app, &admin, &phone).await;
    assert_eq!(unit(&units, &serials[4])["status"], "in_stock");
    assert_eq!(unit(&units, &serials[0])["status"], "returned_faulty");
}

#[tokio::test]
async fn warranty_claims_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn warranty_claims_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}

#[tokio::test]
async fn process_claim_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    process_claim_scenario(&app).await;
}

#[tokio::test]
async fn process_claim_on_mongo() {
    let app = common::spawn_app_isolated().await;
    process_claim_scenario(&app).await;
    app.mongo().drop().await.ok();
}
