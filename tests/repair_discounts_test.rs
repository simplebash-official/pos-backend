//! A discount on a repair item, end to end on SQLite and on a throwaway Atlas database: the live
//! preview, what the server refuses, the job's price after discounts, the invoice line carrying
//! the full price and the ticket's discount (plus the cashier's extra), the net price on the
//! item's warranty.

mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::middleware::auth::Claims;
use simplebash_pos_backend::domain::users::Role;
use tower::ServiceExt;

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

fn admin_token(app: &common::TestApp) -> String {
    let claims = Claims {
        sub: "admin".to_string(),
        exp: 9_999_999_999,
        role: Some(Role::Admin),
        permissions: roles::default_permissions(Role::Admin)
            .iter()
            .map(|p| p.to_string())
            .collect(),
        tid: None,
        scope: None,
        did: None,
        name: Some("Owner Olivia".to_string()),
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

fn no_cover() -> Value {
    json!({ "coverage": { "kind": "none" } })
}

fn months(value: u32) -> Value {
    json!({ "coverage": { "kind": "limited", "value": value, "unit": "months" }, "warrantyType": "shop" })
}

fn job(customer_key: &str, items: Value) -> Value {
    json!({
        "customer": { "customerKey": customer_key },
        "deviceModel": "Galaxy A55",
        "serialNumber": format!("DEV{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
        "issueDescription": "Cracked screen",
        "items": items,
    })
}

fn sale(lines: Value) -> Value {
    json!({
        "staff": { "cashierName": "Cashier Kamal" },
        "customer": { "customerName": "Nimal Perera", "customerPhone": "077 123 4567" },
        "items": lines,
        "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 10_000_000 },
        "shopProfileSnapshot": { "tradingName": "Repair Test Shop" },
    })
}

async fn scenario(app: &common::TestApp) {
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the admin is seeded");
    let admin = admin_token(app);
    let customer = common::register_customer(app, "Nimal Perera", "0771234567").await;

    // A screen in stock at one price tier (cost 4,000, sold at 12,000).
    let (status, category) = send(
        app,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Parts {}", uuid::Uuid::new_v4()), "icon": "Box", "color": "blue",
            "subcategories": ["Spares"],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{category}");
    let (status, screen) = send(
        app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": format!("Screen {}", &uuid::Uuid::new_v4().simple().to_string()[..6]),
            "categoryKey": text(&category["data"]["key"]),
            "subcategoryKey": text(&category["data"]["subcategories"][0]["key"]),
            "minStockThreshold": 1,
            "warranty": no_cover(),
            "openingStock": { "quantity": 3, "unitCostCents": 4_000, "sellingPriceCents": 12_000 },
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{screen}");
    let screen = text(&screen["data"]["key"]);
    let screen_part =
        json!({ "source": "stock", "productKey": screen, "sellingPriceCents": 12_000 });

    // ---- The live preview works each discount out and blocks one that cannot stand ----
    let (status, preview) = send(
        app,
        "POST",
        "/api/calc/repair",
        Some(json!({ "items": [
            { "partCents": 12_000, "partCostCents": 4_000,
              "discount": { "type": "percentage", "basisPoints": 1_250 } },
            { "laborCents": 5_000, "discount": { "type": "fixed", "cents": 500 } },
            { "laborCents": 100, "discount": { "type": "fixed", "cents": 200 } },
        ] })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(
        preview["data"]["itemDiscountsCents"],
        json!([1_500, 500, 0])
    );
    assert_eq!(
        preview["data"]["blockers"],
        json!([{ "itemIndex": 2, "code": "discount_more_than_item" }])
    );
    assert_eq!(preview["data"]["totals"]["discountCents"], 2_000);
    assert_eq!(
        preview["data"]["totals"]["priceCents"],
        12_000 + 5_000 + 100 - 2_000
    );

    // ---- Refused discounts ----
    for (items, what) in [
        (
            json!([{ "description": "Fee", "laborCents": 1_000, "warranty": no_cover(),
                     "discount": { "type": "fixed", "cents": 1_001 } }]),
            "more than the fee",
        ),
        (
            json!([{ "description": "Fee", "laborCents": 1_000, "warranty": no_cover(),
                     "discount": { "type": "percentage", "basisPoints": 0 } }]),
            "a percentage of nothing",
        ),
        (
            json!([{ "description": "Fee", "laborCents": 1_000, "warranty": no_cover(),
                     "discount": { "type": "percentage", "basisPoints": 10_001 } }]),
            "over 100%",
        ),
        (
            json!([{ "description": "Fee", "laborCents": 0, "warranty": no_cover(),
                     "discount": { "type": "fixed", "cents": 1 } }]),
            "an item with no price",
        ),
        (
            json!([{ "description": "Fee and part", "laborCents": 1_000, "warranty": no_cover(),
                     "part": { "source": "outside", "name": "Glue", "priceCents": 500, "costCents": 100 },
                     "discount": { "type": "fixed", "cents": 100 } }]),
            "an item with both a fee and a part",
        ),
    ] {
        let (status, body) = send(
            app,
            "POST",
            "/api/repairs",
            Some(job(&customer, items)),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: {body}");
    }

    // ---- A job with a discounted screen (12.5%) and a discounted, covered fee (Rs 5 off) ----
    let (status, created) = send(
        app,
        "POST",
        "/api/repairs",
        Some(job(
            &customer,
            json!([
                { "description": "Replace screen", "laborCents": 0, "warranty": no_cover(),
                  "part": screen_part, "discount": { "type": "percentage", "basisPoints": 1_250 } },
                { "description": "Fit and test", "laborCents": 5_000, "warranty": months(3),
                  "discount": { "type": "fixed", "cents": 500 } },
            ]),
        )),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let repair = created["data"].clone();
    let repair_key = text(&repair["key"]);
    assert_eq!(repair["items"][0]["discount"]["type"], "percentage");
    assert_eq!(repair["items"][0]["discount"]["basisPoints"], 1_250);
    assert_eq!(repair["items"][0]["discount"]["amountCents"], 1_500);
    assert_eq!(repair["items"][1]["discount"]["type"], "fixed");
    assert_eq!(repair["items"][1]["discount"]["cents"], 500);
    assert_eq!(repair["items"][1]["discount"]["amountCents"], 500);
    assert_eq!(
        repair["priceCents"],
        12_000 + 5_000 - 2_000,
        "the job's price is what the customer pays"
    );
    assert_eq!(repair["costCents"], 4_000);

    // An edit that sends the discount back keeps it; one that leaves it out removes it.
    let (status, body) = send(
        app,
        "PATCH",
        &format!("/api/repairs/{repair_key}"),
        Some(json!({ "items": [
            { "key": repair["items"][0]["key"], "description": "Replace screen", "laborCents": 0,
              "warranty": no_cover(), "part": screen_part },
            { "key": repair["items"][1]["key"], "description": "Fit and test", "laborCents": 5_000,
              "warranty": months(3), "discount": { "type": "fixed", "cents": 500 } },
        ] })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["items"][0].get("discount").is_none());
    assert_eq!(body["data"]["priceCents"], 12_000 + 5_000 - 500);
    let (status, body) = send(
        app,
        "PATCH",
        &format!("/api/repairs/{repair_key}"),
        Some(json!({ "items": [
            { "key": repair["items"][0]["key"], "description": "Replace screen", "laborCents": 0,
              "warranty": no_cover(), "part": screen_part,
              "discount": { "type": "percentage", "basisPoints": 1_250 } },
            { "key": repair["items"][1]["key"], "description": "Fit and test", "laborCents": 5_000,
              "warranty": months(3), "discount": { "type": "fixed", "cents": 500 } },
        ], "status": "ready" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["priceCents"], 15_000);

    // ---- Billing it: the invoice line carries the full price and the ticket's discount ----
    for (quantity, extra, what) in [
        (2, 0, "billed twice"),
        (1, 15_001, "an extra above the price"),
    ] {
        let (status, body) = send(
            app,
            "POST",
            "/api/billing/sales",
            Some(sale(json!([{ "quantity": quantity, "discountCents": extra,
                               "sourceType": "repair", "sourceTicketKey": repair_key,
                               "handover": "now" }]))),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: {body}");
    }
    let (status, sold) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(sale(json!([{ "quantity": 1, "discountCents": 1_000,
                           "sourceType": "repair", "sourceTicketKey": repair_key,
                               "handover": "now" }]))),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    let line = &sold["data"]["invoice"]["items"][0];
    assert_eq!(
        line["unitPriceCents"], 17_000,
        "the price before the ticket's discounts"
    );
    assert_eq!(
        line["discountCents"],
        2_000 + 1_000,
        "the ticket's discount and the cashier's extra"
    );
    assert_eq!(line["totalCents"], 14_000);
    assert_eq!(sold["data"]["invoice"]["totalCents"], 14_000);

    // ---- The fee's warranty is priced at what the customer paid for it ----
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/warranties?repairKey={repair_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let issued = body["data"]["items"].as_array().expect("an array");
    assert_eq!(issued.len(), 1, "only the fee has cover: {body}");
    assert_eq!(issued[0]["unitPriceCents"], 5_000 - 500);
}

#[tokio::test]
async fn repair_discounts_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn repair_discounts_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}
