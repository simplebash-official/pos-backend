//! Repair jobs as items, end to end on SQLite and on a throwaway Atlas database: the shop's
//! ready-made services, items with labour, stock parts (a serial or a price tier, priced on the
//! server) and outside parts, the job totals, billing a job (parts leave stock, each covered item
//! gets its own warranty against the device serial), the locks after hand-back, a warranty claim
//! on repair work opening a free rework job, handing that rework back, and both migrations of
//! the old two-amount jobs.

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

fn no_cover() -> Value {
    json!({ "coverage": { "kind": "none" } })
}

fn months(value: u32) -> Value {
    json!({ "coverage": { "kind": "limited", "value": value, "unit": "months" }, "warrantyType": "shop" })
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

/// A new product's key and id.
async fn product(app: &common::TestApp, admin: &str, body: Value) -> (String, String) {
    let (status, created) = send(app, "POST", "/api/inventory/products", Some(body), admin).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    (text(&created["data"]["key"]), text(&created["data"]["id"]))
}

async fn stock_of(app: &common::TestApp, token: &str, product: &(String, String)) -> i64 {
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/inventory/products/{}", product.1),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"]["stockQuantity"]
        .as_i64()
        .expect("the stock is a number")
}

async fn repair_services(app: &common::TestApp, admin: &str, manager: &str, staff: &str) {
    let service = json!({
        "name": "Screen replacement", "laborCents": 2_500, "warranty": months(3), "active": true,
    });
    let (status, body) = send(
        app,
        "POST",
        "/api/repair-services",
        Some(service.clone()),
        staff,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "staff cannot add services: {body}"
    );
    let (status, created) = send(
        app,
        "POST",
        "/api/repair-services",
        Some(service.clone()),
        manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let key = text(&created["data"]["key"]);
    assert!(key.starts_with("rst_"), "{created}");

    // The same name with other case and spacing is the same service.
    let mut twin = service.clone();
    twin["name"] = json!("  screen  REPLACEMENT ");
    let (status, body) = send(app, "POST", "/api/repair-services", Some(twin), admin).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "REPAIR_SERVICE_NAME_TAKEN");

    let mut bad = service.clone();
    bad["name"] = json!("Battery swap");
    bad["laborCents"] = json!(-1);
    let (status, _) = send(app, "POST", "/api/repair-services", Some(bad), admin).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let mut uncovered_type = service.clone();
    uncovered_type["name"] = json!("Battery swap");
    uncovered_type["warranty"] =
        json!({ "coverage": { "kind": "limited", "value": 1, "unit": "months" } });
    let (status, _) = send(
        app,
        "POST",
        "/api/repair-services",
        Some(uncovered_type),
        admin,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a covered service needs a warranty type"
    );

    let (status, list) = send(app, "GET", "/api/repair-services", None, staff).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(items_of(&list).len(), 1);
    assert_eq!(list["data"][0]["laborCents"], 2_500);

    // Hidden: gone from the form, still listed for whoever manages services.
    let mut hidden = service.clone();
    hidden["active"] = json!(false);
    hidden["laborCents"] = json!(3_000);
    let (status, saved) = send(
        app,
        "PUT",
        &format!("/api/repair-services/{key}"),
        Some(hidden),
        manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["data"]["version"], 2);
    let (_, list) = send(app, "GET", "/api/repair-services", None, staff).await;
    assert!(items_of(&list).is_empty(), "{list}");
    let (status, _) = send(
        app,
        "GET",
        "/api/repair-services?includeHidden=true",
        None,
        staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, list) = send(
        app,
        "GET",
        "/api/repair-services?includeHidden=true",
        None,
        admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["data"][0]["active"], false);
    let (status, _) = send(
        app,
        "PUT",
        "/api/repair-services/rst_missing",
        Some(service),
        admin,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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

fn items_of(body: &Value) -> Vec<Value> {
    body["data"]
        .as_array()
        .expect("the value is an array")
        .clone()
}

fn job(device_serial: &str, items: Value) -> Value {
    json!({
        "customer": { "customerName": "Nimal Perera", "customerPhone": "0771234567" },
        "deviceModel": "Galaxy A55",
        "serialNumber": device_serial,
        "issueDescription": "Cracked screen and weak battery",
        "items": items,
    })
}

async fn scenario(app: &common::TestApp) {
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the admin is seeded");
    let admin = token_for(app, "admin", Role::Admin, "Owner Olivia");
    let manager = user_with_pin(app, &admin, "mgr", (Role::Manager, "manager"), MANAGER_PIN).await;
    let staff = user_with_pin(app, &admin, "kamal", (Role::Staff, "staff"), "2468").await;

    repair_services(app, &admin, &manager, &staff).await;

    // Parts: screens by price tier (cost 4,000, sold at 12,000) and serialized batteries.
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
    let base = |name: &str| {
        json!({
            "name": format!("{name} {}", &uuid::Uuid::new_v4().simple().to_string()[..6]),
            "categoryKey": text(&category["data"]["key"]),
            "subcategoryKey": text(&category["data"]["subcategories"][0]["key"]),
            "minStockThreshold": 1,
            "warranty": months(1),
        })
    };
    let mut screen_body = base("Screen");
    screen_body["openingStock"] =
        json!({ "quantity": 3, "unitCostCents": 4_000, "sellingPriceCents": 12_000 });
    let screen_product = product(app, &admin, screen_body).await;
    let screen = screen_product.0.clone();
    let mut battery_body = base("Battery");
    battery_body["isSerialized"] = json!(true);
    let battery_product = product(app, &admin, battery_body).await;
    let battery = battery_product.0.clone();
    let (status, supplier) = send(
        app,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": format!("Supplier {}", uuid::Uuid::new_v4()), "contactPerson": "Jane",
            "primaryPhone": "+94770000001", "address": "Colombo", "suppliedCategories": ["Parts"],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{supplier}");
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let cells: Vec<String> = (0..2).map(|i| format!("BAT{}{i}", &tag[..10])).collect();
    let (status, body) = send(
        app,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": text(&supplier["data"]["key"]),
            "date": chrono::Utc::now().to_rfc3339(),
            "lines": [{
                "productKey": battery, "quantity": 2, "lineTotalCents": 4_000,
                "sellingPriceCents": 6_000, "serialNumbers": cells,
            }],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // Live totals for the form.
    let (status, totals) = send(
        app,
        "POST",
        "/api/calc/repair",
        Some(json!({ "items": [
            { "laborCents": 2_500, "partCents": 12_000, "partCostCents": 4_000 },
            { "laborCents": 500 },
        ] })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{totals}");
    assert_eq!(totals["data"]["priceCents"], 15_000);
    assert_eq!(totals["data"]["costCents"], 4_000);

    // ---- Refused items ----
    let device = format!("DEV{}", &tag[..12]);
    for (items, code) in [
        (
            json!([{ "description": "Screen", "laborCents": 0, "warranty": no_cover(),
                     "part": { "source": "stock", "productKey": screen, "sellingPriceCents": 99_999 } }]),
            "PRICE_TIER_UNAVAILABLE",
        ),
        (
            json!([{ "description": "Battery", "laborCents": 0, "warranty": no_cover(),
                     "part": { "source": "stock", "productKey": battery, "sellingPriceCents": 6_000 } }]),
            "REPAIR_PART_INVALID",
        ),
        (
            json!([
                { "description": "A", "laborCents": 0, "warranty": no_cover(),
                  "part": { "source": "stock", "productKey": battery, "serialNumber": cells[0] } },
                { "description": "B", "laborCents": 0, "warranty": no_cover(),
                  "part": { "source": "stock", "productKey": battery, "serialNumber": cells[0] } },
            ]),
            "REPAIR_PART_INVALID",
        ),
        (
            json!([{ "key": "rit_not_here", "description": "Screen", "laborCents": 0, "warranty": no_cover() }]),
            "VALIDATION_ERROR",
        ),
        (
            json!([{ "description": "  ", "laborCents": 0, "warranty": no_cover() }]),
            "VALIDATION_ERROR",
        ),
    ] {
        let (status, body) = send(
            app,
            "POST",
            "/api/repairs",
            Some(job(&device, items)),
            &staff,
        )
        .await;
        assert!(status.is_client_error(), "{body}");
        assert_eq!(body["code"], code, "{body}");
    }

    // ---- A job with a stock screen, a stock battery and a cleaning ----
    let (status, created) = send(
        app,
        "POST",
        "/api/repairs",
        Some(job(
            &device,
            json!([
                { "description": "Replace screen", "laborCents": 2_500, "warranty": months(3),
                  "part": { "source": "stock", "productKey": screen, "sellingPriceCents": 12_000 } },
                { "description": "Replace battery", "laborCents": 1_000, "warranty": months(6),
                  "part": { "source": "stock", "productKey": battery, "serialNumber": cells[0] } },
                { "description": "Clean ports", "laborCents": 500, "warranty": no_cover(),
                  "part": { "source": "outside", "name": "Brush", "priceCents": 200, "costCents": 50 } },
            ]),
        )),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let repair = created["data"].clone();
    let repair_key = text(&repair["key"]);
    assert_eq!(
        repair["priceCents"],
        2_500 + 12_000 + 1_000 + 6_000 + 500 + 200
    );
    assert_eq!(repair["costCents"], 4_000 + 2_000 + 50);
    assert_eq!(repair["items"][0]["partCents"], 12_000);
    assert_eq!(repair["items"][0]["partCostCents"], 4_000);
    assert_eq!(repair["items"][0]["part"]["source"], "stock");
    assert_eq!(
        repair["items"][1]["part"]["serialNumber"],
        cells[0].as_str()
    );
    assert_eq!(repair["items"][1]["partCostCents"], 2_000);
    let screen_item = text(&repair["items"][0]["key"]);
    let clean_item = text(&repair["items"][2]["key"]);

    // Editing keeps the keys of items sent back with them, and works the totals out again.
    let mut edited = repair["items"].clone();
    edited[2] = json!({ "key": clean_item, "description": "Clean ports", "laborCents": 800,
                        "warranty": no_cover() });
    for item in edited.as_array_mut().expect("items").iter_mut().take(2) {
        let part = item["part"].clone();
        item["part"] = match part["serialNumber"].as_str() {
            Some(serial) => {
                json!({ "source": "stock", "productKey": part["productKey"], "serialNumber": serial })
            }
            None => {
                json!({ "source": "stock", "productKey": part["productKey"], "sellingPriceCents": part["sellingPriceCents"] })
            }
        };
        for field in ["partCents", "partCostCents"] {
            item.as_object_mut().expect("an object").remove(field);
        }
    }
    let (status, updated) = send(
        app,
        "PATCH",
        &format!("/api/repairs/{repair_key}"),
        Some(json!({ "items": edited, "status": "ready" })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["data"]["items"][0]["key"], screen_item.as_str());
    assert_eq!(updated["data"]["items"][2]["key"], clean_item.as_str());
    assert_eq!(
        updated["data"]["priceCents"],
        2_500 + 12_000 + 1_000 + 6_000 + 800
    );
    assert_eq!(updated["data"]["costCents"], 6_000);

    // A job with stock parts is handed back by billing it, never by hand.
    let (status, body) = send(
        app,
        "PATCH",
        &format!("/api/repairs/{repair_key}"),
        Some(json!({ "status": "delivered" })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "REPAIR_PART_INVALID");
    assert_eq!(
        stock_of(app, &staff, &screen_product).await,
        3,
        "nothing left stock yet"
    );

    // ---- Billed: parts leave stock, covered items get warranties on the device ----
    let bill = sale(
        json!([{ "quantity": 1, "discountCents": 0, "sourceType": "repair",
                             "sourceTicketKey": repair_key }]),
    );
    let (status, sold) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(bill.clone()),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    assert_eq!(sold["data"]["invoice"]["totalCents"], 22_300);
    assert_eq!(sold["data"]["invoice"]["items"][0]["unitCostCents"], 6_000);
    assert!(
        sold["data"]["warnings"]
            .as_array()
            .is_none_or(|w| w.is_empty()),
        "{sold}"
    );
    assert_eq!(stock_of(app, &staff, &screen_product).await, 2);
    assert_eq!(stock_of(app, &staff, &battery_product).await, 1);
    let (status, serial) = send(
        app,
        "GET",
        &format!("/api/inventory/serials/by-number/{}", cells[0]),
        None,
        &staff,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "the used battery is no longer in stock: {serial}"
    );

    let (status, body) = send(
        app,
        "GET",
        &format!("/api/warranties?repairKey={repair_key}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let issued = body["data"]["items"].as_array().expect("an array").clone();
    assert_eq!(issued.len(), 2, "the cleaning has no cover: {body}");
    let screen_warranty = issued
        .iter()
        .find(|w| w["repairItemKey"] == screen_item.as_str())
        .expect("the screen has a warranty")
        .clone();
    assert_eq!(screen_warranty["sourceType"], "repair");
    assert_eq!(screen_warranty["serialNumber"], device.as_str());
    assert_eq!(screen_warranty["itemName"], "Replace screen (Galaxy A55)");
    assert_eq!(screen_warranty["unitPriceCents"], 14_500);
    assert_eq!(screen_warranty["state"], "active");
    let starts = chrono::NaiveDate::parse_from_str(&text(&screen_warranty["startsOn"]), "%Y-%m-%d")
        .expect("a date");
    let expected_end = starts
        .checked_add_months(chrono::Months::new(3))
        .and_then(|d| d.pred_opt())
        .expect("a date");
    assert_eq!(screen_warranty["endsOn"], expected_end.to_string());
    let (status, found) = send(
        app,
        "GET",
        &format!("/api/warranties/lookup?q={device}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(
        found["data"]["groups"][0]["warranties"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );

    // Handed back: not billed twice, and its work no longer changes.
    let (status, body) = send(app, "POST", "/api/billing/sales", Some(bill), &staff).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, body) = send(
        app,
        "PATCH",
        &format!("/api/repairs/{repair_key}"),
        Some(json!({ "items": [] })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "REPAIR_ITEMS_LOCKED");

    // ---- A claim on the screen work becomes a free rework job ----
    let (status, claim) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(json!({
            "warrantyKey": text(&screen_warranty["key"]),
            "issueDescription": "Touch stops working",
            "voidChecks": { "screenCracked": false, "liquidDamage": false, "sealBroken": false, "tampered": false },
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claim}");
    let claim_key = text(&claim["data"]["key"]);
    let (status, body) = send(
        app,
        "POST",
        &format!("/api/warranty-claims/{claim_key}/rework"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "only once decided: {body}");
    for (path, body) in [
        ("inspect", json!({ "inspectionNotes": "Digitizer loose" })),
        (
            "decide",
            json!({ "decision": "repair", "approval": { "managerPin": MANAGER_PIN, "reason": "Covered" } }),
        ),
    ] {
        let (status, answer) = send(
            app,
            "POST",
            &format!("/api/warranty-claims/{claim_key}/{path}"),
            Some(body),
            &staff,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{path}: {answer}");
    }
    let (status, reworked) = send(
        app,
        "POST",
        &format!("/api/warranty-claims/{claim_key}/rework"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reworked}");
    let rework_key = text(&reworked["data"]["reworkRepairKey"]);
    let (status, again) = send(
        app,
        "POST",
        &format!("/api/warranty-claims/{claim_key}/rework"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");

    let (status, rework) = send(
        app,
        "GET",
        &format!("/api/repairs/{rework_key}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rework}");
    assert_eq!(rework["data"]["kind"]["kind"], "warranty_rework");
    assert_eq!(rework["data"]["kind"]["claimKey"], claim_key.as_str());
    assert_eq!(
        rework["data"]["kind"]["originalRepairKey"],
        repair_key.as_str()
    );
    assert_eq!(rework["data"]["serialNumber"], device.as_str());
    assert_eq!(rework["data"]["issueDescription"], "Touch stops working");

    // A rework charges nothing and gives no new warranty, but a stock part still costs.
    let screen_part =
        json!({ "source": "stock", "productKey": screen, "sellingPriceCents": 12_000 });
    for items in [
        json!([{ "description": "Refit screen", "laborCents": 100, "warranty": no_cover() }]),
        json!([{ "description": "Refit screen", "laborCents": 0, "warranty": months(1) }]),
        json!([{ "description": "Refit screen", "laborCents": 0, "warranty": no_cover(),
                 "part": { "source": "outside", "name": "Glue", "priceCents": 10, "costCents": 10 } }]),
    ] {
        let (status, body) = send(
            app,
            "PATCH",
            &format!("/api/repairs/{rework_key}"),
            Some(json!({ "items": items })),
            &staff,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    let (status, body) = send(
        app,
        "PATCH",
        &format!("/api/repairs/{rework_key}"),
        Some(
            json!({ "items": [{ "description": "Replace screen again", "laborCents": 0,
                                 "warranty": no_cover(), "part": screen_part }],
                     "status": "ready" }),
        ),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["priceCents"], 0);
    assert_eq!(body["data"]["costCents"], 4_000);
    assert_eq!(body["data"]["items"][0]["partCents"], 0);

    let (status, body) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(sale(
            json!([{ "quantity": 1, "discountCents": 0, "sourceType": "repair",
                           "sourceTicketKey": rework_key }]),
        )),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "REPAIR_IS_REWORK");

    let (status, body) = send(
        app,
        "PATCH",
        &format!("/api/repairs/{rework_key}"),
        Some(json!({ "status": "delivered" })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        stock_of(app, &staff, &screen_product).await,
        1,
        "the rework part left stock"
    );
    let (_, body) = send(
        app,
        "GET",
        &format!("/api/warranties?repairKey={rework_key}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(
        body["data"]["total"], 0,
        "a rework gives no new warranty: {body}"
    );

    // ---- A claim on a product sale has no repair to rework ----
    let (status, sold) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(sale(
            json!([{ "productKey": screen, "quantity": 1, "discountCents": 0,
                           "sourceType": "retail", "sellingPriceCents": 12_000 }]),
        )),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    let invoice_key = text(&sold["data"]["invoice"]["key"]);
    let (_, sold_warranties) = send(
        app,
        "GET",
        &format!("/api/warranties?invoiceKey={invoice_key}"),
        None,
        &staff,
    )
    .await;
    let (status, claim) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(json!({
            "warrantyKey": text(&sold_warranties["data"]["items"][0]["key"]),
            "issueDescription": "Dead pixels",
            "voidChecks": { "screenCracked": false, "liquidDamage": false, "sealBroken": false, "tampered": false },
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claim}");
    let product_claim = text(&claim["data"]["key"]);
    for (path, body) in [
        ("inspect", json!({ "inspectionNotes": "Panel fault" })),
        (
            "decide",
            json!({ "decision": "repair", "approval": { "managerPin": MANAGER_PIN, "reason": "Covered" } }),
        ),
    ] {
        let (status, answer) = send(
            app,
            "POST",
            &format!("/api/warranty-claims/{product_claim}/{path}"),
            Some(body),
            &staff,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{path}: {answer}");
    }
    let (status, body) = send(
        app,
        "POST",
        &format!("/api/warranty-claims/{product_claim}/rework"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "CLAIM_REWORK_INVALID");
}

#[tokio::test]
async fn repair_items_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn repair_items_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}

#[tokio::test]
async fn an_old_sqlite_database_moves_to_repair_items() {
    use simplebash_pos_backend::clients::sqlite;
    use simplebash_pos_backend::core::logging::SqlLogging;
    use sqlx::Executor;

    let path = std::env::temp_dir().join(format!("pos_rep_mig_{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("the database opens");
    // Put the old two-amount table back on top of a fresh schema.
    for statement in [
        "DROP TABLE repairs",
        "CREATE TABLE repairs (key TEXT PRIMARY KEY, id TEXT NOT NULL, ticket_number TEXT NOT NULL \
         UNIQUE, customer_key TEXT, customer_name TEXT NOT NULL, customer_phone TEXT NOT NULL, \
         device_model TEXT NOT NULL, serial_number TEXT, issue_description TEXT NOT NULL, \
         promised_ready_at TEXT, status TEXT NOT NULL, estimated_cost_cents INTEGER, \
         material_cost_cents INTEGER, assigned_employee_id TEXT, assigned_employee_name TEXT, \
         split_type TEXT, split_value REAL, version INTEGER NOT NULL DEFAULT 1, created_at TEXT \
         NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT, updated_by_device TEXT)",
        "INSERT INTO repairs (key, id, ticket_number, customer_name, customer_phone, device_model, \
         issue_description, status, estimated_cost_cents, material_cost_cents, created_at, \
         updated_at) VALUES ('rep_quoted', '65f1a1a1a1a1a1a1a1a1a1e1', 'REP-000001', 'Ama', \
         '0771234567', 'iPhone 12', 'Broken port', 'delivered', 400000, 150000, \
         '2026-03-10T05:00:00+00:00', '2026-03-10T05:00:00+00:00')",
        "INSERT INTO repairs (key, id, ticket_number, customer_name, customer_phone, device_model, \
         issue_description, status, created_at, updated_at) VALUES ('rep_new', \
         '65f1a1a1a1a1a1a1a1a1a1e2', 'REP-000002', 'Kasun', '0771234568', 'Pixel 7', 'No power', \
         'received', '2026-03-11T05:00:00+00:00', '2026-03-11T05:00:00+00:00')",
    ] {
        pool.execute(statement)
            .await
            .expect("the old shape is set up");
    }

    sqlite::init_db(&pool).await.expect("the migration runs");

    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('repairs')")
        .fetch_all(&pool)
        .await
        .expect("columns read");
    for gone in ["estimated_cost_cents", "material_cost_cents"] {
        assert!(!columns.iter().any(|c| c == gone), "{gone} is dropped");
    }
    let rows: Vec<(String, String, String, i64, i64)> = sqlx::query_as(
        "SELECT key, kind, items, price_cents, cost_cents FROM repairs ORDER BY key",
    )
    .fetch_all(&pool)
    .await
    .expect("repairs read");
    let (_, kind, items, price, cost) = &rows[1];
    assert_eq!(rows[1].0, "rep_quoted");
    assert_eq!((*price, *cost), (400_000, 150_000));
    assert_eq!(
        serde_json::from_str::<Value>(kind).expect("json")["kind"],
        "standard"
    );
    let items: Value = serde_json::from_str(items).expect("json");
    assert_eq!(items[0]["key"], "rit_mig_rep_quoted");
    assert_eq!(items[0]["laborCents"], 400_000);
    assert_eq!(items[0]["partCostCents"], 150_000);
    assert_eq!(items[0]["part"]["source"], "outside");
    assert_eq!(rows[0].0, "rep_new");
    assert_eq!(rows[0].2, "[]");
    assert_eq!((rows[0].3, rows[0].4), (0, 0));

    // The rows load as jobs, and a second start changes nothing.
    let db = simplebash_pos_backend::clients::db::Db::Sqlite(pool.clone());
    let job = simplebash_pos_backend::modules::repairs::service::get_repair(&db, "rep_quoted")
        .await
        .expect("the job loads");
    assert_eq!(job.price_cents, 400_000);
    sqlite::init_db(&pool).await.expect("the second start runs");
    drop(pool);
    std::fs::remove_file(&path).ok();
}

#[tokio::test]
async fn an_old_mongo_database_moves_to_repair_items() {
    use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
    use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
    use simplebash_pos_backend::modules::repairs::migration::migrate_mongo;

    let app = common::spawn_app_isolated().await;
    let db = app.mongo();
    let repairs = db.collection::<Document>("repairs");
    let now = BsonDateTime::now();
    for (key, estimated, material) in [
        ("rep_old_a", Some(400_000i64), Some(150_000i64)),
        ("rep_old_b", None, None),
    ] {
        let mut row = doc! {
            "key": key, "ticket_number": format!("REP-{key}"), "customer_name": "Ama",
            "customer_phone": "0771234567", "device_model": "iPhone 12",
            "issue_description": "Broken port", "status": "received", "version": 1_i64,
            "created_at": now, "updated_at": now,
        };
        if let Some(cents) = estimated {
            row.insert("estimated_cost_cents", cents);
        }
        if let Some(cents) = material {
            row.insert("material_cost_cents", cents as i32);
        }
        repairs
            .insert_one(row)
            .await
            .expect("the old job is stored");
    }

    let dry = migrate_mongo(db, Mode::DryRun)
        .await
        .expect("the dry run runs");
    assert_eq!((dry.repairs, dry.with_items), (2, 1));
    assert!(
        repairs
            .find_one(doc! { "key": "rep_old_a" })
            .await
            .expect("read")
            .expect("found")
            .contains_key("estimated_cost_cents"),
        "a dry run writes nothing"
    );

    migrate_mongo(db, Mode::Apply)
        .await
        .expect("the migration applies");
    let migrated = repairs
        .find_one(doc! { "key": "rep_old_a" })
        .await
        .expect("read")
        .expect("found");
    assert!(!migrated.contains_key("estimated_cost_cents"));
    assert!(!migrated.contains_key("material_cost_cents"));
    assert_eq!(migrated.get_i64("price_cents").expect("price"), 400_000);
    assert_eq!(migrated.get_i64("cost_cents").expect("cost"), 150_000);
    let job =
        simplebash_pos_backend::modules::repairs::service::get_repair(&app.db_handle, "rep_old_b")
            .await
            .expect("the job loads");
    assert!(job.items.is_empty());

    let again = migrate_mongo(db, Mode::Apply)
        .await
        .expect("the rerun runs");
    assert_eq!(again.repairs, 0, "idempotent");
    db.drop().await.ok();
}
