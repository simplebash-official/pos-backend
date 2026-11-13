//! Opening several claims of one invoice in one request, on SQLite and on a throwaway Atlas
//! database: the request refused as a whole before anything is written, one claim per item with
//! its own text and units, a failed item beside an opened one, and the manager approval for
//! warranties that ran out (checked once, only for those).

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

fn token_with(
    app: &common::TestApp,
    user: &str,
    role: Role,
    name: &str,
    permissions: Vec<String>,
) -> String {
    let claims = Claims {
        sub: user.to_string(),
        exp: 9_999_999_999,
        role: Some(role),
        permissions,
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

fn token_for(app: &common::TestApp, user: &str, role: Role, name: &str) -> String {
    let permissions = roles::default_permissions(role)
        .iter()
        .map(|p| p.to_string())
        .collect();
    token_with(app, user, role, name, permissions)
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

/// Sells `lines` to one customer and returns the invoice key.
async fn buy(app: &common::TestApp, token: &str, lines: Value) -> String {
    let (status, sold) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier Kamal" },
            "customer": { "customerName": "Nimal Perera", "customerPhone": "077 123 4567" },
            "items": lines,
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 10_000_000 },
            "shopProfileSnapshot": { "tradingName": "Warranty Test Shop" },
        })),
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    text(&sold["data"]["invoice"]["key"])
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

async fn claims_of_invoice(app: &common::TestApp, token: &str, invoice_key: &str) -> Vec<Value> {
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/warranty-claims?status=all&invoiceKey={invoice_key}"),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    items(&body)
}

fn item(warranty_key: &str, issue: &str) -> Value {
    json!({ "warrantyKey": warranty_key, "issueDescription": issue })
}

async fn batch(
    app: &common::TestApp,
    token: &str,
    items: Vec<Value>,
    approval: Option<Value>,
) -> (StatusCode, Value) {
    let mut body = json!({ "items": items });
    if let Some(approval) = approval {
        body["approval"] = approval;
    }
    send(app, "POST", "/api/warranty-claims/batch", Some(body), token).await
}

fn results(body: &Value) -> Vec<Value> {
    body["data"]["results"]
        .as_array()
        .expect("the results are an array")
        .clone()
}

/// The number after `WCL-`.
fn claim_sequence(claim: &Value) -> u64 {
    text(&claim["claimNumber"])
        .strip_prefix("WCL-")
        .expect("a claim number starts with WCL-")
        .parse()
        .expect("a claim number ends in digits")
}

/// Moves a warranty's end date three days back, straight in the store.
async fn run_out(app: &common::TestApp, key: &str) {
    let past = (chrono::Utc::now() - chrono::Duration::days(3))
        .date_naive()
        .to_string();
    match &app.db_handle {
        simplebash_pos_backend::clients::db::Db::Sqlite(pool) => {
            sqlx::query("UPDATE warranties SET ends_on = ? WHERE key = ?")
                .bind(&past)
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
                    mongodb::bson::doc! { "$set": { "ends_on": &past } },
                )
                .await
                .expect("the end date moves");
        }
    }
}

fn error_of(result: &Value) -> (u64, String) {
    (
        result["error"]["status"].as_u64().expect("a status"),
        text(&result["error"]["code"]),
    )
}

async fn scenario(app: &common::TestApp) {
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the admin is seeded");
    let admin = token_for(app, "admin", Role::Admin, "Owner Olivia");
    let manager = user_with_pin(app, &admin, "mgr", (Role::Manager, "manager"), MANAGER_PIN).await;
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
    let serials: Vec<String> = (0..8).map(|i| format!("IMEI{}{i}", &tag[..10])).collect();
    let (status, body) = send(
        app,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": text(&supplier["data"]["key"]),
            "date": chrono::Utc::now().to_rfc3339(),
            "lines": [{
                "productKey": phone, "quantity": 8, "lineTotalCents": 400_000,
                "sellingPriceCents": 90_000, "serialNumbers": serials,
            }],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let phone_line = |first: usize| {
        json!({ "productKey": phone, "quantity": 2, "discountCents": 0, "sourceType": "retail",
                "serialNumbers": [serials[first], serials[first + 1]] })
    };
    let charger_line = |quantity: i64| {
        json!({ "productKey": charger, "quantity": quantity, "discountCents": 0,
                "sourceType": "retail", "sellingPriceCents": 1500 })
    };
    let key_of = |warranties: &[Value], serial: Option<&str>| {
        text(
            &warranties
                .iter()
                .find(|w| w["serialNumber"].as_str() == serial)
                .expect("the unit has a warranty")["key"],
        )
    };

    // ---- The request is wrong as a whole: refused, nothing written ----
    let invoice = buy(app, &staff, json!([phone_line(0), charger_line(2)])).await;
    let issued = warranties_of(app, &staff, &invoice).await;
    let phone_a = key_of(&issued, Some(&serials[0]));
    let phone_b = key_of(&issued, Some(&serials[1]));
    let charger_w = key_of(&issued, None);
    let other_invoice = buy(app, &staff, json!([charger_line(1)])).await;
    let other_w = key_of(&warranties_of(app, &staff, &other_invoice).await, None);

    let (status, body) = batch(app, &staff, vec![], None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let too_many = (0..101)
        .map(|n| item(&format!("wty_{n}"), "Fault"))
        .collect();
    let (status, body) = batch(app, &staff, too_many, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = batch(
        app,
        &staff,
        vec![item(&phone_a, "First"), item(&phone_a, "Again")],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = batch(
        app,
        &staff,
        vec![item(&phone_a, "Fault"), item("wty_missing", "Fault")],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = batch(
        app,
        &staff,
        vec![item(&phone_a, "Fault"), item(&other_w, "Fault")],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        text(&body["message"]).contains("different invoices"),
        "{body}"
    );
    let (status, body) = batch(
        app,
        &staff,
        vec![item(&phone_a, "Fault"), item(&phone_b, "Fault")],
        Some(json!({ "managerPin": MANAGER_PIN, "reason": "Nobody needs this" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(claims_of_invoice(app, &staff, &invoice).await.is_empty());
    assert!(
        claims_of_invoice(app, &staff, &other_invoice)
            .await
            .is_empty()
    );

    // Someone who may not record claims is turned away.
    let nobody = token_with(app, "nobody", Role::Staff, "Nobody", Vec::new());
    let (status, body) = batch(app, &nobody, vec![item(&phone_a, "Fault")], None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // ---- Three claims at once, each with its own text and units ----
    let mut charger_item = item(&charger_w, "  Gets hot  ");
    charger_item["quantity"] = json!(2);
    let (status, body) = batch(
        app,
        &staff,
        vec![
            item(&phone_a, "Screen flickers"),
            item(&phone_b, "No sound"),
            charger_item,
        ],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["openedCount"], 3, "{body}");
    assert_eq!(body["data"]["failedCount"], 0, "{body}");
    let answers = results(&body);
    let order = [&phone_a, &phone_b, &charger_w];
    for (answer, expected) in answers.iter().zip(order) {
        assert_eq!(answer["outcome"], "opened", "{answer}");
        assert_eq!(answer["warrantyKey"], expected.as_str(), "{answer}");
        assert_eq!(
            answer["claim"]["warrantyKey"],
            expected.as_str(),
            "{answer}"
        );
    }
    assert_eq!(answers[0]["claim"]["issueDescription"], "Screen flickers");
    assert_eq!(answers[1]["claim"]["issueDescription"], "No sound");
    assert_eq!(answers[2]["claim"]["issueDescription"], "Gets hot");
    assert_eq!(answers[2]["claim"]["quantity"], 2);
    assert_eq!(answers[0]["claim"]["serialNumber"], serials[0].as_str());
    let sequence: Vec<u64> = answers
        .iter()
        .map(|a| claim_sequence(&a["claim"]))
        .collect();
    assert!(
        sequence.windows(2).all(|pair| pair[0] < pair[1]),
        "{sequence:?}"
    );
    for warranty in warranties_of(app, &staff, &invoice).await {
        assert_eq!(warranty["state"], "under_claim", "{warranty}");
    }
    assert_eq!(claims_of_invoice(app, &staff, &invoice).await.len(), 3);

    // ---- One item cannot be opened: it fails alone ----
    let third = buy(app, &staff, json!([phone_line(2)])).await;
    let third_w = warranties_of(app, &staff, &third).await;
    let (p3, p4) = (
        key_of(&third_w, Some(&serials[2])),
        key_of(&third_w, Some(&serials[3])),
    );
    let (status, single) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(item(&p3, "Already here")),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{single}");
    let (status, body) = batch(
        app,
        &staff,
        vec![item(&p3, "Again"), item(&p4, "Fault")],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["openedCount"], 1, "{body}");
    assert_eq!(body["data"]["failedCount"], 1, "{body}");
    let answers = results(&body);
    assert_eq!(answers[0]["outcome"], "failed", "{body}");
    assert_eq!(
        error_of(&answers[0]),
        (409, "CLAIM_ALREADY_OPEN".to_string())
    );
    assert_eq!(answers[1]["outcome"], "opened", "{body}");
    assert_eq!(claims_of_invoice(app, &staff, &third).await.len(), 2);

    // ---- A warranty that ran out, Staff without an approval ----
    let fourth = buy(app, &staff, json!([phone_line(4)])).await;
    let fourth_w = warranties_of(app, &staff, &fourth).await;
    let (expired, in_force) = (
        key_of(&fourth_w, Some(&serials[4])),
        key_of(&fourth_w, Some(&serials[5])),
    );
    run_out(app, &expired).await;
    let (status, body) = batch(
        app,
        &staff,
        vec![item(&expired, "Fault"), item(&in_force, "Fault")],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let answers = results(&body);
    assert_eq!(error_of(&answers[0]), (409, "WARRANTY_EXPIRED".to_string()));
    assert_eq!(answers[1]["outcome"], "opened", "{body}");

    // ---- Staff with a manager's PIN: checked once, honours only the one that ran out ----
    let fifth = buy(app, &staff, json!([phone_line(6)])).await;
    let fifth_w = warranties_of(app, &staff, &fifth).await;
    let (expired, in_force) = (
        key_of(&fifth_w, Some(&serials[6])),
        key_of(&fifth_w, Some(&serials[7])),
    );
    run_out(app, &expired).await;
    let items_of = || vec![item(&expired, "Fault"), item(&in_force, "Fault")];
    let (status, body) = batch(
        app,
        &staff,
        items_of(),
        Some(json!({ "managerPin": "0000", "reason": "Customer waited" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(claims_of_invoice(app, &staff, &fifth).await.is_empty());
    let (status, body) = batch(
        app,
        &staff,
        items_of(),
        Some(json!({ "managerPin": MANAGER_PIN, "reason": "Customer waited" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["openedCount"], 2, "{body}");
    let answers = results(&body);
    assert!(
        answers[0]["claim"]["expiredOverrideByName"].is_string(),
        "{body}"
    );
    assert!(
        answers[1]["claim"]["expiredOverrideByName"].is_null(),
        "{body}"
    );

    // ---- Admin and Manager approve for themselves and must send no PIN ----
    for (token, send_pin) in [(&manager, true), (&admin, false)] {
        let invoice = buy(app, &staff, json!([charger_line(1)])).await;
        let warranty = key_of(&warranties_of(app, &staff, &invoice).await, None);
        run_out(app, &warranty).await;
        if send_pin {
            let (status, body) = batch(
                app,
                token,
                vec![item(&warranty, "Fault")],
                Some(json!({ "managerPin": MANAGER_PIN, "reason": "Mine" })),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert!(claims_of_invoice(app, token, &invoice).await.is_empty());
        }
        let (status, body) = batch(app, token, vec![item(&warranty, "Fault")], None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let answers = results(&body);
        assert_eq!(answers[0]["outcome"], "opened", "{body}");
        assert!(
            answers[0]["claim"]["expiredOverrideByName"].is_string(),
            "{body}"
        );
    }
}

#[tokio::test]
async fn claims_batch_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn claims_batch_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}
