//! The warranty dashboard and report end to end, on SQLite and on a throwaway Atlas database:
//! the desk's headline numbers (`/warranties/stats`), the "ending this week" list, late claims
//! in the dashboard's reminders, and the warranty report with its permission.

mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use chrono::{Duration, NaiveDate, Utc};
use serde_json::{Value, json};
use simplebash_pos_backend::clients::db::Db;
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::middleware::auth::Claims;
use simplebash_pos_backend::core::utils::shop_day;
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
        // Axum's own query rejections answer in plain text.
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, json)
}

/// Sends a request that must succeed and returns its `data`.
async fn ok(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> Value {
    let (status, answer) = send(app, method, uri, body, token).await;
    assert!(status.is_success(), "{method} {uri}: {status} {answer}");
    answer["data"].clone()
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

/// A user of `role` with `pin`, and a token for them.
async fn user_with_pin(
    app: &common::TestApp,
    admin: &str,
    name: &str,
    role: (Role, &str),
    pin: &str,
) -> String {
    let user = ok(
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
    let token = token_for(app, &text(&user["id"]), role.0, name);
    ok(
        app,
        "POST",
        "/api/auth/set-pin",
        Some(json!({ "pin": pin })),
        &token,
    )
    .await;
    token
}

fn day_text(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

fn checks() -> Value {
    json!({ "screenCracked": false, "liquidDamage": false, "sealBroken": false, "tampered": false })
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

/// A sale that bills a job, to the registered customer the job is for.
fn ticket_sale(customer: &str, lines: Value) -> Value {
    let mut body = sale(lines);
    body["customer"] = json!({ "customerKey": customer });
    body
}

/// Moves a claim's arrival day, and when given its supplier's promised day, straight in the
/// store: the API never accepts a day in the past.
async fn backdate_claim(
    app: &common::TestApp,
    key: &str,
    received_on: &str,
    expected_back_on: Option<&str>,
) {
    match &app.db_handle {
        Db::Sqlite(pool) => {
            sqlx::query("UPDATE warranty_claims SET received_on = ? WHERE key = ?")
                .bind(received_on)
                .bind(key)
                .execute(pool)
                .await
                .expect("the arrival day moves");
            if let Some(expected) = expected_back_on {
                sqlx::query(
                    "UPDATE warranty_claims \
                     SET supplier = json_set(supplier, '$.expectedBackOn', ?) WHERE key = ?",
                )
                .bind(expected)
                .bind(key)
                .execute(pool)
                .await
                .expect("the promised day moves");
            }
        }
        Db::Mongo(_) => {
            let mut set = mongodb::bson::doc! { "received_on": received_on };
            if let Some(expected) = expected_back_on {
                set.insert("supplier.expectedBackOn", expected);
            }
            app.mongo()
                .collection::<mongodb::bson::Document>("warranty_claims")
                .update_one(
                    mongodb::bson::doc! { "key": key },
                    mongodb::bson::doc! { "$set": set },
                )
                .await
                .expect("the claim moves");
        }
    }
}

async fn scenario(app: &common::TestApp) {
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the admin is seeded");
    let admin = token_for(app, "admin", Role::Admin, "Owner Olivia");
    let manager = user_with_pin(app, &admin, "mgr", (Role::Manager, "manager"), MANAGER_PIN).await;
    let staff = user_with_pin(app, &admin, "kamal", (Role::Staff, "staff"), "2468").await;
    let today = shop_day(Utc::now());

    // An empty desk: every number is zero.
    let empty = ok(app, "GET", "/api/warranties/stats", None, &staff).await;
    assert_eq!(
        empty,
        json!({
            "inForceCount": 0, "endingSoonCount": 0,
            "endingSoonUntil": day_text(today + Duration::days(6)),
            "openClaimCount": 0, "lateClaimCount": 0,
            "reworkCostThisMonthCents": 0,
        })
    );

    // ---- A phone (12 months, serialized) and a charger (3 days) ----
    let category = ok(
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
    let base = |name: &str| {
        json!({
            "name": format!("{name} {}", &uuid::Uuid::new_v4().simple().to_string()[..6]),
            "categoryKey": text(&category["key"]),
            "subcategoryKey": text(&category["subcategories"][0]["key"]),
            "minStockThreshold": 1,
        })
    };
    let mut phone_body = base("Phone");
    phone_body["isSerialized"] = json!(true);
    phone_body["warranty"] = json!({
        "coverage": { "kind": "limited", "value": 12, "unit": "months" },
        "warrantyType": "shop",
    });
    let phone_product = ok(
        app,
        "POST",
        "/api/inventory/products",
        Some(phone_body),
        &admin,
    )
    .await;
    let phone = text(&phone_product["key"]);
    let phone_name = text(&phone_product["name"]);
    let mut charger_body = base("Charger");
    charger_body["openingStock"] =
        json!({ "quantity": 10, "unitCostCents": 500, "sellingPriceCents": 1500 });
    charger_body["warranty"] = json!({
        "coverage": { "kind": "limited", "value": 3, "unit": "days" },
        "warrantyType": "shop",
    });
    let charger_product = ok(
        app,
        "POST",
        "/api/inventory/products",
        Some(charger_body),
        &admin,
    )
    .await;
    let charger = text(&charger_product["key"]);
    let charger_name = text(&charger_product["name"]);

    let supplier = ok(
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
    let supplier_key = text(&supplier["key"]);
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let serials: Vec<String> = (0..2).map(|i| format!("IMEI{}{i}", &tag[..10])).collect();
    ok(
        app,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": supplier_key,
            "date": Utc::now().to_rfc3339(),
            "lines": [{
                "productKey": phone, "quantity": 2, "lineTotalCents": 100_000,
                "sellingPriceCents": 90_000, "serialNumbers": serials,
            }],
        })),
        &admin,
    )
    .await;
    let sold = ok(
        app,
        "POST",
        "/api/billing/sales",
        Some(sale(json!([
            { "productKey": phone, "quantity": 2, "discountCents": 0, "sourceType": "retail",
              "serialNumbers": [serials[0], serials[1]] },
            { "productKey": charger, "quantity": 1, "discountCents": 0, "sourceType": "retail",
              "sellingPriceCents": 1500 },
        ]))),
        &staff,
    )
    .await;
    let invoice_key = text(&sold["invoice"]["key"]);
    let issued = ok(
        app,
        "GET",
        &format!("/api/warranties?invoiceKey={invoice_key}"),
        None,
        &staff,
    )
    .await["items"]
        .as_array()
        .expect("an array")
        .clone();
    let key_of = |serial: Option<&str>| {
        text(
            &issued
                .iter()
                .find(|w| w["serialNumber"].as_str() == serial)
                .expect("the unit has a warranty")["key"],
        )
    };
    let phone_a = key_of(Some(&serials[0]));
    let charger_w = key_of(None);

    // ---- A repair by a technician, billed, so its screen work carries a warranty ----
    let repair_customer = common::register_customer(app, "Nimal Perera", "0771234567").await;
    let technician = ok(
        app,
        "POST",
        "/api/employees",
        Some(
            json!({ "name": format!("Tech {}", &tag[..6]), "phone": "0771234567",
                     "role": "technician", "defaultSplitType": "percentage",
                     "defaultSplitValue": 20.0 }),
        ),
        &admin,
    )
    .await;
    let repair = ok(
        app,
        "POST",
        "/api/repairs",
        Some(json!({
            "customer": { "customerKey": repair_customer },
            "deviceModel": "Galaxy A55",
            "serialNumber": format!("DEV{}", &tag[..8]),
            "issueDescription": "Cracked screen",
            "status": "ready",
            "assignment": { "assignedEmployeeId": text(&technician["key"]) },
            "items": [{
                "description": "Replace screen", "laborCents": 1_000,
                "warranty": { "coverage": { "kind": "limited", "value": 3, "unit": "months" },
                              "warrantyType": "shop" },
                "part": { "source": "outside", "name": "Screen", "priceCents": 5_000,
                          "costCents": 3_000 },
            }],
        })),
        &staff,
    )
    .await;
    let repair_key = text(&repair["key"]);
    ok(
        app,
        "POST",
        "/api/billing/sales",
        Some(ticket_sale(
            &repair_customer,
            json!([{ "quantity": 1, "discountCents": 0, "sourceType": "repair",
                           "sourceTicketKey": repair_key, "handover": "now" }]),
        )),
        &staff,
    )
    .await;
    let repair_w = ok(
        app,
        "GET",
        &format!("/api/warranties?repairKey={repair_key}"),
        None,
        &staff,
    )
    .await["items"][0]
        .clone();
    let repair_w_key = text(&repair_w["key"]);
    let repair_item_name = text(&repair_w["itemName"]);

    // ---- Claim 1: phone A sent to the supplier, who is now late ----
    let claim_body = |warranty: &str| json!({ "warrantyKey": warranty, "issueDescription": "Dead", "voidChecks": checks() });
    let claim_1 = text(
        &ok(
            app,
            "POST",
            "/api/warranty-claims",
            Some(claim_body(&phone_a)),
            &staff,
        )
        .await["key"],
    );
    ok(
        app,
        "POST",
        &format!("/api/warranty-claims/{claim_1}/inspect"),
        Some(json!({ "inspectionNotes": "Board fault" })),
        &staff,
    )
    .await;
    ok(
        app,
        "POST",
        &format!("/api/warranty-claims/{claim_1}/supplier/send"),
        Some(json!({ "supplierKey": supplier_key,
                     "expectedBackOn": day_text(today + Duration::days(2)) })),
        &staff,
    )
    .await;
    let promised = day_text(today - Duration::days(3));
    backdate_claim(app, &claim_1, &day_text(today), Some(&promised)).await;

    // ---- Claim 2: the screen work, reworked for free, open ten days ----
    let claim_2 = text(
        &ok(
            app,
            "POST",
            "/api/warranty-claims",
            Some(claim_body(&repair_w_key)),
            &staff,
        )
        .await["key"],
    );
    ok(
        app,
        "POST",
        &format!("/api/warranty-claims/{claim_2}/inspect"),
        Some(json!({ "inspectionNotes": "Digitizer loose" })),
        &staff,
    )
    .await;
    ok(
        app,
        "POST",
        &format!("/api/warranty-claims/{claim_2}/decide"),
        Some(json!({ "decision": "repair",
                     "approval": { "managerPin": MANAGER_PIN, "reason": "Covered" } })),
        &staff,
    )
    .await;
    let rework_key = text(
        &ok(
            app,
            "POST",
            &format!("/api/warranty-claims/{claim_2}/rework"),
            None,
            &staff,
        )
        .await["reworkRepairKey"],
    );
    let reworked = ok(
        app,
        "PATCH",
        &format!("/api/repairs/{rework_key}"),
        Some(json!({ "items": [{
            "description": "Refit screen", "laborCents": 0,
            "warranty": { "coverage": { "kind": "none" } },
            "part": { "source": "outside", "name": "Glue", "priceCents": 0, "costCents": 150 },
        }] })),
        &staff,
    )
    .await;
    assert_eq!(reworked["costCents"], 150);
    backdate_claim(app, &claim_2, &day_text(today - Duration::days(10)), None).await;

    // ---- The desk: phone B and the charger in force, the charger ending this week ----
    let stats = ok(app, "GET", "/api/warranties/stats", None, &staff).await;
    assert_eq!(
        stats,
        json!({
            "inForceCount": 2, "endingSoonCount": 1,
            "endingSoonUntil": day_text(today + Duration::days(6)),
            "openClaimCount": 2, "lateClaimCount": 2,
            "reworkCostThisMonthCents": 150,
        }),
        "phone A and the screen work are held by their claims"
    );

    let ending = ok(
        app,
        "GET",
        "/api/warranties?state=expiring_soon",
        None,
        &staff,
    )
    .await;
    assert_eq!(ending["total"], 1, "{ending}");
    assert_eq!(ending["items"][0]["key"], charger_w.as_str());
    assert_eq!(ending["items"][0]["state"], "expiring_soon");
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/warranties?state=expiring_soon&invoiceKey={invoice_key}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "one filter at a time: {body}"
    );
    let (status, body) = send(app, "GET", "/api/warranties?state=expired", None, &staff).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "an unknown state: {body}");

    // ---- Late claims reach the dashboard's attention list ----
    let reminders = ok(
        app,
        "GET",
        "/api/reports/reminders?limit=100",
        None,
        &manager,
    )
    .await;
    assert_eq!(reminders["claimsLateCount"], 2, "{reminders}");
    let entry = |key: &str| {
        reminders["reminders"]
            .as_array()
            .expect("an array")
            .iter()
            .find(|r| r["key"] == key)
            .unwrap_or_else(|| panic!("claim {key} is listed: {reminders}"))
            .clone()
    };
    let late_supplier = entry(&claim_1);
    assert_eq!(late_supplier["kind"], "claim_supplier_late");
    assert_eq!(late_supplier["dueDate"], promised.as_str());
    assert_eq!(late_supplier["daysFromDue"], 3);
    assert_eq!(late_supplier["severity"], "warning");
    assert_eq!(late_supplier["title"], phone_name.as_str());
    assert_eq!(late_supplier["customerName"], "Nimal Perera");
    assert_eq!(late_supplier["amountCents"], 0);
    assert_eq!(
        late_supplier["linkTo"],
        format!("/warranties?view=claims&claim={claim_1}")
    );
    assert!(text(&late_supplier["referenceNumber"]).starts_with("WCL-"));
    let slow = entry(&claim_2);
    assert_eq!(slow["kind"], "claim_slow");
    assert_eq!(slow["dueDate"], day_text(today - Duration::days(3)));
    assert_eq!(slow["daysFromDue"], 3);

    // ---- The report: four warranties given, two claimed ----
    let (status, body) = send(
        app,
        "GET",
        "/api/reports/warranties?preset=this_month",
        None,
        &staff,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "staff see no reports: {body}"
    );
    let report = ok(
        app,
        "GET",
        "/api/reports/warranties?preset=this_month",
        None,
        &manager,
    )
    .await;
    assert_eq!(report["issuedCount"], 4, "{report}");
    assert_eq!(report["claimedCount"], 2);
    assert_eq!(report["claimRateBps"], 5_000);
    assert_eq!(
        report["byProduct"],
        json!([
            { "name": phone_name, "issuedCount": 2, "claimedCount": 1, "claimRateBps": 5_000 },
            { "name": charger_name, "issuedCount": 1, "claimedCount": 0, "claimRateBps": 0 },
        ])
    );
    assert_eq!(
        report["byRepairService"],
        json!([{ "name": repair_item_name, "issuedCount": 1, "claimedCount": 1,
                 "claimRateBps": 10_000 }])
    );
    assert_eq!(
        report["byTechnician"],
        json!([{ "name": technician["name"], "issuedCount": 1, "claimedCount": 1,
                 "claimRateBps": 10_000 }])
    );
    let suppliers = report["bySupplier"].as_array().expect("an array");
    assert_eq!(suppliers.len(), 1, "{report}");
    assert_eq!(suppliers[0]["supplierKey"], supplier_key.as_str());
    assert_eq!(suppliers[0]["supplierName"], supplier["name"]);
    assert_eq!(suppliers[0]["sentCount"], 1);
    assert_eq!(suppliers[0]["awayCount"], 1);
    assert_eq!(suppliers[0]["overdueCount"], 1);
    assert!(
        suppliers[0].get("averageDaysAway").is_none(),
        "nothing came back yet: {report}"
    );
    assert_eq!(
        report["reworkByMonth"],
        json!([{ "month": today.format("%Y-%m").to_string(), "jobCount": 1, "costCents": 150 }])
    );
    assert_eq!(report["reworkJobCount"], 1);
    assert_eq!(report["reworkCostCents"], 150);

    // A period with nothing in it.
    let empty = ok(
        app,
        "GET",
        "/api/reports/warranties?preset=custom&from=2020-01-01&to=2020-01-31",
        None,
        &admin,
    )
    .await;
    assert_eq!(empty["issuedCount"], 0);
    assert_eq!(empty["claimRateBps"], 0);
    assert_eq!(empty["byProduct"], json!([]));
    assert_eq!(empty["bySupplier"], json!([]));
    assert_eq!(empty["reworkByMonth"], json!([]));
    assert_eq!(empty["reworkCostCents"], 0);
}

#[tokio::test]
async fn warranty_dashboard_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn warranty_dashboard_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}
