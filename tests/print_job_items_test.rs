//! Print jobs as items, end to end on SQLite and on a throwaway Atlas database: the shop's
//! ready-made print services, lines with quantities (service fees, stock parts by price tier and
//! by serial, outside parts) priced on the server, the staff note, the locks and hand-back
//! rules, billing (parts leave stock once, the invoice total is the job's price), warranties per
//! covered line, claims (a free rework print job, a paid print job), the warranty report, and a
//! credit note that returns a whole print line.

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

/// Sends a request that must succeed, and returns its `data`.
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

/// Sends a request that must be refused with this status and error code.
async fn refused(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
    expected: (StatusCode, &str),
) {
    let (status, answer) = send(app, method, uri, body, token).await;
    assert_eq!(status, expected.0, "{method} {uri}: {answer}");
    assert_eq!(answer["code"], expected.1, "{method} {uri}: {answer}");
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

fn no_cover() -> Value {
    json!({ "coverage": { "kind": "none" } })
}

fn months(value: u32) -> Value {
    json!({ "coverage": { "kind": "limited", "value": value, "unit": "months" }, "warrantyType": "shop" })
}

fn service(description: &str, quantity: i64, unit_price_cents: i64) -> Value {
    json!({ "description": description, "quantity": quantity, "warranty": no_cover(),
            "source": { "type": "service", "unitPriceCents": unit_price_cents } })
}

/// The shop one scenario runs in.
struct Shop {
    admin: String,
    manager: String,
    staff: String,
    customer: String,
    /// A mug sold at 500 a unit, costing 300: (key, id).
    mug: (String, String),
    /// A serialized frame sold at 6,000, costing 2,000.
    frame: (String, String),
    serials: Vec<String>,
}

impl Shop {
    async fn open(app: &common::TestApp) -> Shop {
        simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
            .await
            .expect("the admin is seeded");
        let admin = token_for(app, "admin", Role::Admin, "Owner Olivia");
        let customer = common::register_customer(app, "Nimal Perera", "0771234567").await;
        let manager =
            user_with_pin(app, &admin, "mgr", (Role::Manager, "manager"), MANAGER_PIN).await;
        let staff = user_with_pin(app, &admin, "kamal", (Role::Staff, "staff"), "2468").await;
        let category = ok(
            app,
            "POST",
            "/api/inventory/categories",
            Some(json!({
                "name": format!("Blanks {}", uuid::Uuid::new_v4()), "icon": "Box", "color": "blue",
                "subcategories": ["Print"],
            })),
            &admin,
        )
        .await;
        let base = |name: &str| {
            json!({
                "name": format!("{name} {}", &uuid::Uuid::new_v4().simple().to_string()[..6]),
                "categoryKey": text(&category["key"]),
                "subcategoryKey": text(&category["subcategories"][0]["key"]),
                "warranty": months(1),
            })
        };
        let mut mug_body = base("Mug");
        mug_body["openingStock"] =
            json!({ "quantity": 150, "unitCostCents": 300, "sellingPriceCents": 500 });
        let mug = ok(
            app,
            "POST",
            "/api/inventory/products",
            Some(mug_body),
            &admin,
        )
        .await;
        let mut frame_body = base("Frame");
        frame_body["isSerialized"] = json!(true);
        let frame = ok(
            app,
            "POST",
            "/api/inventory/products",
            Some(frame_body),
            &admin,
        )
        .await;
        let supplier = ok(
            app,
            "POST",
            "/api/suppliers",
            Some(json!({
                "name": format!("Supplier {}", uuid::Uuid::new_v4()), "contactPerson": "Jane",
                "primaryPhone": "+94770000001", "address": "Colombo", "suppliedCategories": ["Print"],
            })),
            &admin,
        )
        .await;
        let tag = uuid::Uuid::new_v4().simple().to_string();
        let serials: Vec<String> = (0..3).map(|i| format!("FRM{}{i}", &tag[..10])).collect();
        ok(
            app,
            "POST",
            "/api/purchase-receipts",
            Some(json!({
                "supplierKey": text(&supplier["key"]),
                "date": chrono::Utc::now().to_rfc3339(),
                "lines": [{
                    "productKey": text(&frame["key"]), "quantity": 3, "lineTotalCents": 6_000,
                    "sellingPriceCents": 6_000, "serialNumbers": serials,
                }],
            })),
            &admin,
        )
        .await;
        Shop {
            admin,
            manager,
            staff,
            customer,
            mug: (text(&mug["key"]), text(&mug["id"])),
            frame: (text(&frame["key"]), text(&frame["id"])),
            serials,
        }
    }

    fn mugs(&self, quantity: i64, warranty: Value) -> Value {
        json!({ "description": "White mug", "quantity": quantity, "warranty": warranty,
                "source": { "type": "stock", "productKey": self.mug.0, "sellingPriceCents": 500 } })
    }

    fn framed(&self, serial: &str, quantity: i64, warranty: Value) -> Value {
        json!({ "description": "Photo frame", "quantity": quantity, "warranty": warranty,
                "source": { "type": "stock", "productKey": self.frame.0, "serialNumber": serial } })
    }

    fn job(&self, items: Value) -> Value {
        json!({
            "customer": { "customerKey": self.customer },
            "jobType": "mug", "quantity": 100, "items": items,
        })
    }

    async fn stock(&self, app: &common::TestApp, product: &(String, String)) -> i64 {
        ok(
            app,
            "GET",
            &format!("/api/inventory/products/{}", product.1),
            None,
            &self.staff,
        )
        .await["stockQuantity"]
            .as_i64()
            .expect("the stock is a number")
    }

    async fn read(&self, app: &common::TestApp, key: &str) -> Value {
        ok(
            app,
            "GET",
            &format!("/api/print-jobs/{key}"),
            None,
            &self.staff,
        )
        .await
    }

    fn sale(&self, job_key: &str, handover: &str) -> Value {
        json!({
            "staff": { "cashierName": "Cashier Kamal" },
            "customer": { "customerKey": self.customer },
            "items": [{ "quantity": 1, "discountCents": 0, "sourceType": "print",
                        "sourceTicketKey": job_key, "handover": handover }],
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 10_000_000 },
            "shopProfileSnapshot": { "tradingName": "Print Test Shop" },
        })
    }

    async fn warranties(&self, app: &common::TestApp, job_key: &str) -> Vec<Value> {
        ok(
            app,
            "GET",
            &format!("/api/warranties?printJobKey={job_key}"),
            None,
            &self.staff,
        )
        .await["items"]
            .as_array()
            .expect("an array")
            .clone()
    }

    /// A claim on a warranty, inspected and decided; returns its key.
    async fn claim(&self, app: &common::TestApp, warranty_key: &str, decision: &str) -> String {
        let claim = ok(
            app,
            "POST",
            "/api/warranty-claims",
            Some(json!({
                "warrantyKey": warranty_key,
                "issueDescription": "Print is peeling",
                "voidChecks": { "screenCracked": false, "liquidDamage": false, "sealBroken": false, "tampered": false },
            })),
            &self.staff,
        )
        .await;
        let key = text(&claim["key"]);
        for (path, body) in [
            ("inspect", json!({ "inspectionNotes": "Confirmed" })),
            (
                "decide",
                json!({ "decision": decision, "approval": { "managerPin": MANAGER_PIN, "reason": "Checked" } }),
            ),
        ] {
            ok(
                app,
                "POST",
                &format!("/api/warranty-claims/{key}/{path}"),
                Some(body),
                &self.staff,
            )
            .await;
        }
        key
    }
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

async fn print_services(app: &common::TestApp, shop: &Shop) {
    let service = json!({
        "name": "Mug printing", "unitPriceCents": 150, "warranty": months(3), "active": true,
    });
    refused(
        app,
        "POST",
        "/api/print-services",
        Some(service.clone()),
        &shop.staff,
        (StatusCode::FORBIDDEN, "PERMISSION_DENIED"),
    )
    .await;
    let created = ok(
        app,
        "POST",
        "/api/print-services",
        Some(service.clone()),
        &shop.manager,
    )
    .await;
    let key = text(&created["key"]);
    assert!(key.starts_with("pst_"), "{created}");

    // The same name with other case and spacing is the same service.
    let mut twin = service.clone();
    twin["name"] = json!("  mug  PRINTING ");
    refused(
        app,
        "POST",
        "/api/print-services",
        Some(twin),
        &shop.admin,
        (StatusCode::CONFLICT, "PRINT_SERVICE_NAME_TAKEN"),
    )
    .await;
    let mut negative = service.clone();
    negative["name"] = json!("Design");
    negative["unitPriceCents"] = json!(-1);
    refused(
        app,
        "POST",
        "/api/print-services",
        Some(negative),
        &shop.admin,
        (StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
    )
    .await;

    let mut hidden = service.clone();
    hidden["active"] = json!(false);
    ok(
        app,
        "PUT",
        &format!("/api/print-services/{key}"),
        Some(hidden),
        &shop.manager,
    )
    .await;
    let offered = ok(app, "GET", "/api/print-services", None, &shop.staff).await;
    assert_eq!(offered.as_array().map(Vec::len), Some(0), "{offered}");
    let all = ok(
        app,
        "GET",
        "/api/print-services?includeHidden=true",
        None,
        &shop.manager,
    )
    .await;
    assert_eq!(all.as_array().map(Vec::len), Some(1), "{all}");
    refused(
        app,
        "GET",
        "/api/print-services?includeHidden=true",
        None,
        &shop.staff,
        (StatusCode::FORBIDDEN, "PERMISSION_DENIED"),
    )
    .await;
}

async fn refused_items(app: &common::TestApp, shop: &Shop) {
    let bad_request = (StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
    let part_invalid = (StatusCode::BAD_REQUEST, "PRINT_JOB_PART_INVALID");
    let mut over_discount = service("Design", 2, 50);
    over_discount["discount"] = json!({ "type": "fixed", "cents": 101 });
    let mut unknown_key = service("Design", 1, 50);
    unknown_key["key"] = json!("pit_not_here");
    for (items, expected) in [
        (json!([service("Design", 0, 50)]), bad_request),
        (json!([service("Design", 1_000_001, 50)]), bad_request),
        (json!([service("   ", 1, 50)]), bad_request),
        (json!([service("Design", 1, -1)]), bad_request),
        (json!([over_discount]), bad_request),
        (json!([unknown_key]), bad_request),
        (
            json!([shop.framed(&shop.serials[0], 2, no_cover())]),
            part_invalid,
        ),
        (
            json!([
                shop.framed(&shop.serials[0], 1, no_cover()),
                shop.framed(&shop.serials[0], 1, no_cover()),
            ]),
            part_invalid,
        ),
        // 100 and 51 each fit the 150 in stock, but not together.
        (
            json!([shop.mugs(100, no_cover()), shop.mugs(51, no_cover())]),
            (StatusCode::CONFLICT, "INSUFFICIENT_STOCK"),
        ),
    ] {
        refused(
            app,
            "POST",
            "/api/print-jobs",
            Some(shop.job(items)),
            &shop.staff,
            expected,
        )
        .await;
    }
    let mut ready = shop.job(json!([]));
    ready["status"] = json!("ready");
    refused(
        app,
        "POST",
        "/api/print-jobs",
        Some(ready),
        &shop.staff,
        (StatusCode::BAD_REQUEST, "PRINT_JOB_PRICE_REQUIRED"),
    )
    .await;
    let mut long_note = shop.job(json!([]));
    long_note["note"] = json!("x".repeat(1_001));
    refused(
        app,
        "POST",
        "/api/print-jobs",
        Some(long_note),
        &shop.staff,
        bad_request,
    )
    .await;
}

/// A priced job with every kind of line: returns its key.
async fn priced_job(app: &common::TestApp, shop: &Shop) -> String {
    let mut printing = service("Mug printing", 100, 150);
    printing["discount"] = json!({ "type": "percentage", "basisPoints": 1_000 });
    let mut body = shop.job(json!([
        printing,
        shop.mugs(100, months(3)),
        { "description": "Gift box", "quantity": 100, "warranty": no_cover(),
          "source": { "type": "outside", "name": "Gift box", "unitPriceCents": 80, "unitCostCents": 50 } },
        shop.framed(&shop.serials[0], 1, months(6)),
    ]));
    body["note"] = json!("  Blue logo, both sides  ");
    let created = ok(app, "POST", "/api/print-jobs", Some(body), &shop.staff).await;
    // Service 15,000 less 10% + mugs 50,000 + boxes 8,000 + frame 6,000.
    assert_eq!(created["priceCents"], 13_500 + 50_000 + 8_000 + 6_000);
    assert_eq!(created["costCents"], 30_000 + 5_000 + 2_000);
    assert_eq!(created["note"], "Blue logo, both sides");
    assert_eq!(created["kind"]["kind"], "standard");
    let items = created["items"].as_array().expect("the lines");
    assert!(text(&items[0]["key"]).starts_with("pit_"));
    assert_eq!(items[0]["discount"]["amountCents"], 1_500);
    assert_eq!(items[0]["costCents"], 0);
    assert_eq!(items[1]["unitPriceCents"], 500);
    assert_eq!(items[1]["costCents"], 30_000);
    assert_eq!(items[1]["source"]["sellingPriceCents"], 500);
    assert_eq!(items[2]["costCents"], 5_000);
    assert_eq!(items[3]["source"]["serialNumber"], shop.serials[0].as_str());
    assert_eq!(items[3]["costCents"], 2_000);
    assert_eq!(
        shop.stock(app, &shop.mug).await,
        150,
        "quoting moves no stock"
    );
    text(&created["key"])
}

async fn notes(app: &common::TestApp, shop: &Shop, key: &str) {
    let uri = format!("/api/print-jobs/{key}");
    let before = shop.read(app, key).await;
    // A note alone changes only the note.
    let noted = ok(
        app,
        "PATCH",
        &uri,
        Some(json!({ "note": "Collect Friday" })),
        &shop.staff,
    )
    .await;
    assert_eq!(noted["note"], "Collect Friday");
    assert_eq!(noted["items"], before["items"]);
    assert_eq!(noted["priceCents"], before["priceCents"]);
    // Left out, it stays; blank, it goes.
    let kept = ok(
        app,
        "PATCH",
        &uri,
        Some(json!({ "quantity": 100 })),
        &shop.staff,
    )
    .await;
    assert_eq!(kept["note"], "Collect Friday");
    let cleared = ok(
        app,
        "PATCH",
        &uri,
        Some(json!({ "note": "   " })),
        &shop.staff,
    )
    .await;
    assert!(cleared.get("note").is_none(), "{cleared}");
}

/// Billing with the hand-over later, then handing back. Returns the delivered job.
async fn bill_then_hand_back(app: &common::TestApp, shop: &Shop, key: &str) -> Value {
    let uri = format!("/api/print-jobs/{key}");
    refused(
        app,
        "PATCH",
        &uri,
        Some(json!({ "status": "delivered" })),
        &shop.staff,
        (StatusCode::BAD_REQUEST, "PRINT_JOB_PART_INVALID"),
    )
    .await;
    let price = shop.read(app, key).await["priceCents"]
        .as_i64()
        .expect("a price");
    let sold = ok(
        app,
        "POST",
        "/api/billing/sales",
        Some(shop.sale(key, "later")),
        &shop.staff,
    )
    .await;
    assert_eq!(
        sold["invoice"]["totalCents"], price,
        "the bill is the job's price: {sold}"
    );
    assert_eq!(sold["invoice"]["items"][0]["unitPriceCents"], price + 1_500);
    assert_eq!(sold["invoice"]["items"][0]["discountCents"], 1_500);
    assert_eq!(shop.stock(app, &shop.mug).await, 50, "the mugs left stock");
    assert_eq!(shop.stock(app, &shop.frame).await, 2);
    refused(
        app,
        "PATCH",
        &uri,
        Some(json!({ "items": [] })),
        &shop.staff,
        (StatusCode::CONFLICT, "PRINT_JOB_ITEMS_LOCKED"),
    )
    .await;
    assert!(shop.warranties(app, key).await.is_empty());

    let delivered = ok(
        app,
        "PATCH",
        &uri,
        Some(json!({ "status": "delivered" })),
        &shop.staff,
    )
    .await;
    assert_eq!(shop.stock(app, &shop.mug).await, 50, "stock leaves once");
    let warranties = shop.warranties(app, key).await;
    assert_eq!(warranties.len(), 2, "only covered lines: {warranties:?}");
    let ticket = text(&delivered["ticketNumber"]);
    let mug = warranties
        .iter()
        .find(|w| w["itemName"] == format!("White mug x 100 ({ticket})").as_str())
        .expect("the mugs are covered");
    assert_eq!(mug["sourceType"], "print");
    assert_eq!(mug["quantity"], 1);
    assert_eq!(mug["unitPriceCents"], 50_000);
    assert!(mug.get("serialNumber").is_none(), "{mug}");
    let frame = warranties
        .iter()
        .find(|w| w["serialNumber"] == shop.serials[0].as_str())
        .expect("the frame is covered by its serial");
    assert_eq!(frame["printJobKey"], key);

    // Reopened and handed back again, it keeps its first warranties.
    for status in ["ready", "delivered"] {
        ok(
            app,
            "PATCH",
            &uri,
            Some(json!({ "status": status })),
            &shop.staff,
        )
        .await;
    }
    assert_eq!(shop.warranties(app, key).await.len(), 2);
    delivered
}

async fn rework_claim(app: &common::TestApp, shop: &Shop, original: &Value) {
    let key = text(&original["key"]);
    let warranties = shop.warranties(app, &key).await;
    let mug = warranties
        .iter()
        .find(|w| w.get("serialNumber").is_none())
        .expect("the mug warranty");
    let claim = shop.claim(app, &text(&mug["key"]), "repair").await;
    let claim_uri = format!("/api/warranty-claims/{claim}");
    let reworked = ok(
        app,
        "POST",
        &format!("{claim_uri}/rework"),
        None,
        &shop.staff,
    )
    .await;
    assert!(reworked.get("reworkRepairKey").is_none(), "{reworked}");
    let rework_key = text(&reworked["reworkPrintJobKey"]);
    refused(
        app,
        "POST",
        &format!("{claim_uri}/rework"),
        None,
        &shop.staff,
        (StatusCode::CONFLICT, "CLAIM_STEP_NOT_ALLOWED"),
    )
    .await;

    let rework = shop.read(app, &rework_key).await;
    assert_eq!(rework["kind"]["kind"], "warranty_rework");
    assert_eq!(rework["kind"]["claimKey"], claim.as_str());
    assert_eq!(rework["kind"]["originalPrintJobKey"], key.as_str());
    assert_eq!(rework["jobType"], original["jobType"]);
    assert_eq!(rework["quantity"], original["quantity"]);
    assert_eq!(rework["note"], "Print is peeling");

    // A rework charges nothing and gives no new warranty.
    let uri = format!("/api/print-jobs/{rework_key}");
    let mut discounted = service("Reprint", 1, 0);
    discounted["discount"] = json!({ "type": "fixed", "cents": 1 });
    let mut covered = service("Reprint", 1, 0);
    covered["warranty"] = months(1);
    for items in [
        json!([service("Reprint", 1, 100)]),
        json!([discounted]),
        json!([covered]),
    ] {
        refused(
            app,
            "PATCH",
            &uri,
            Some(json!({ "items": items })),
            &shop.staff,
            (StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
        )
        .await;
    }
    let ready = ok(
        app,
        "PATCH",
        &uri,
        Some(json!({
            "items": [service("Reprint", 2, 0), shop.mugs(2, no_cover())],
            "status": "ready",
        })),
        &shop.staff,
    )
    .await;
    assert_eq!(ready["priceCents"], 0);
    assert_eq!(ready["costCents"], 600);
    assert_eq!(ready["items"][1]["unitPriceCents"], 0);
    refused(
        app,
        "POST",
        "/api/billing/sales",
        Some(shop.sale(&rework_key, "now")),
        &shop.staff,
        (StatusCode::CONFLICT, "PRINT_JOB_IS_REWORK"),
    )
    .await;
    ok(
        app,
        "PATCH",
        &uri,
        Some(json!({ "status": "delivered" })),
        &shop.staff,
    )
    .await;
    assert_eq!(
        shop.stock(app, &shop.mug).await,
        48,
        "the rework mugs left stock"
    );
    assert!(shop.warranties(app, &rework_key).await.is_empty());

    let report = ok(
        app,
        "GET",
        "/api/reports/warranties?preset=this_month",
        None,
        &shop.manager,
    )
    .await;
    assert!(
        report["byPrintWork"]
            .as_array()
            .expect("rows")
            .iter()
            .any(|row| row["name"] == "White mug" && row["claimedCount"] == 1),
        "{report}"
    );
    assert_eq!(report["reworkJobCount"], 1, "{report}");
    assert_eq!(report["reworkCostCents"], 600, "{report}");
}

async fn rejected_claim_becomes_a_paid_print_job(
    app: &common::TestApp,
    shop: &Shop,
    original: &Value,
) {
    let key = text(&original["key"]);
    let warranties = shop.warranties(app, &key).await;
    let frame = warranties
        .iter()
        .find(|w| w.get("serialNumber").is_some())
        .expect("the frame warranty");
    let claim = shop.claim(app, &text(&frame["key"]), "reject").await;
    let uri = format!("/api/warranty-claims/{claim}");
    refused(
        app,
        "POST",
        &format!("{uri}/paid-repair"),
        Some(json!({})),
        &shop.staff,
        (StatusCode::CONFLICT, "CLAIM_PAID_JOB_INVALID"),
    )
    .await;
    let paid = ok(
        app,
        "POST",
        &format!("{uri}/paid-print-job"),
        Some(json!({})),
        &shop.staff,
    )
    .await;
    let job = shop.read(app, &text(&paid["paidPrintJobKey"])).await;
    assert_eq!(job["kind"]["kind"], "standard");
    assert_eq!(job["quantity"], original["quantity"]);
    assert_eq!(job["items"].as_array().map(Vec::len), Some(0));
    assert!(
        text(&job["note"]).contains(&text(&paid["claimNumber"])),
        "{job}"
    );
    refused(
        app,
        "POST",
        &format!("{uri}/paid-print-job"),
        Some(json!({})),
        &shop.staff,
        (StatusCode::CONFLICT, "CLAIM_STEP_NOT_ALLOWED"),
    )
    .await;
}

/// A covered serialized line billed with the hand-over now, then wholly refunded.
async fn a_refunded_print_line_loses_its_warranties(app: &common::TestApp, shop: &Shop) {
    let created = ok(
        app,
        "POST",
        "/api/print-jobs",
        Some(shop.job(json!([shop.framed(&shop.serials[1], 1, months(6))]))),
        &shop.staff,
    )
    .await;
    let key = text(&created["key"]);
    let sold = ok(
        app,
        "POST",
        "/api/billing/sales",
        Some(shop.sale(&key, "now")),
        &shop.staff,
    )
    .await;
    assert_eq!(shop.read(app, &key).await["status"], "delivered");
    assert_eq!(shop.warranties(app, &key).await[0]["state"], "active");
    ok(
        app,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": text(&sold["invoice"]["key"]),
            "returnedItems": [{
                "sourceTicketKey": key, "name": text(&sold["invoice"]["items"][0]["name"]),
                "quantity": 1, "reason": "defective", "condition": "resalable",
            }],
            "paymentMethod": "cash",
        })),
        &shop.admin,
    )
    .await;
    assert_eq!(shop.warranties(app, &key).await[0]["state"], "void");
}

async fn photo_framing_job(app: &common::TestApp, shop: &Shop) {
    let created = ok(
        app,
        "POST",
        "/api/print-jobs",
        Some(json!({
            "customer": { "customerKey": shop.customer },
            "jobType": "photo-framing", "quantity": 3,
            "items": [service("8x10 photo print", 3, 45_000)],
        })),
        &shop.staff,
    )
    .await;
    assert_eq!(created["jobType"], "photo-framing");
}

async fn scenario(app: &common::TestApp) {
    let shop = Shop::open(app).await;
    print_services(app, &shop).await;
    refused_items(app, &shop).await;
    let key = priced_job(app, &shop).await;
    notes(app, &shop, &key).await;
    let delivered = bill_then_hand_back(app, &shop, &key).await;
    rework_claim(app, &shop, &delivered).await;
    rejected_claim_becomes_a_paid_print_job(app, &shop, &delivered).await;
    a_refunded_print_line_loses_its_warranties(app, &shop).await;
    photo_framing_job(app, &shop).await;
}

#[tokio::test]
async fn print_job_items_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn print_job_items_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}
