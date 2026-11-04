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
    let repair = open(app, &staff, &charger_key).await;
    let repair_key = text(&repair["key"]);
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
