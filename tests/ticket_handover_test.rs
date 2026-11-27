//! Billing a repair job or print job and handing it back are two separate facts, end to end on
//! SQLite and on a throwaway Atlas database: every ticket line on a sale says whether it is
//! handed over now or later, billing marks the ticket with its invoice (and, for "now", delivers
//! it), a billed ticket cannot be billed again and its work is locked, a job billed with the
//! hand-over later gets its warranties the day it is handed back, voiding the invoice removes the
//! mark, a job billed to anyone but its own customer must be confirmed (and its warranty stays
//! with that customer), and the one-time migration marks tickets billed before the mark existed.

mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use chrono::{FixedOffset, Utc};
use mongodb::bson::{Document, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::clients::db::Db;
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::middleware::auth::Claims;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::modules::billing::migration::{migrate_mongo, migrate_sqlite};
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
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

fn months(value: u32) -> Value {
    json!({ "coverage": { "kind": "limited", "value": value, "unit": "months" }, "warrantyType": "shop" })
}

/// Today in the shop's own time zone (UTC+05:30), as a warranty stores it.
fn shop_today() -> String {
    let offset = FixedOffset::east_opt(5 * 3600 + 1800).expect("a valid offset");
    Utc::now()
        .with_timezone(&offset)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

/// A sale of `lines` by `customer`, paid in cash, or on credit.
fn sale(customer: &str, lines: Value, credit: bool) -> Value {
    let payment = if credit {
        json!({ "paymentMethod": "credit", "isCredit": true })
    } else {
        json!({ "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 10_000_000 })
    };
    json!({
        "staff": { "cashierName": "Cashier Kamal" },
        "customer": { "customerKey": customer },
        "items": lines,
        "payment": payment,
        "shopProfileSnapshot": { "tradingName": "Handover Test Shop" },
    })
}

fn line(ticket_type: &str, ticket_key: &str, handover: Option<&str>) -> Value {
    let mut line = json!({
        "quantity": 1, "discountCents": 0, "sourceType": ticket_type,
        "sourceTicketKey": ticket_key,
    });
    if let Some(handover) = handover {
        line["handover"] = json!(handover);
    }
    line
}

struct Shop {
    admin: String,
    staff: String,
    customer: String,
    screen: (String, String),
}

impl Shop {
    async fn open(app: &common::TestApp) -> Shop {
        simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
            .await
            .expect("the admin is seeded");
        let admin = token_for(app, "admin", Role::Admin, "Owner Olivia");
        let staff = token_for(app, "staff_kamal", Role::Staff, "Cashier Kamal");
        let customer = common::register_customer(app, "Nimal Perera", "0771234567").await;
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
        let (status, created) = send(
            app,
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": format!("Screen {}", &uuid::Uuid::new_v4().simple().to_string()[..6]),
                "categoryKey": text(&category["data"]["key"]),
                "subcategoryKey": text(&category["data"]["subcategories"][0]["key"]),
                "minStockThreshold": 1,
                "warranty": months(1),
                "openingStock": { "quantity": 6, "unitCostCents": 4_000, "sellingPriceCents": 12_000 },
            })),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        Shop {
            admin,
            staff,
            customer,
            screen: (text(&created["data"]["key"]), text(&created["data"]["id"])),
        }
    }

    /// A ready repair job with a covered screen from stock, or (with `from_stock` false) a
    /// covered outside part.
    async fn repair(&self, app: &common::TestApp, from_stock: bool) -> String {
        let part = if from_stock {
            json!({ "source": "stock", "productKey": self.screen.0, "sellingPriceCents": 12_000 })
        } else {
            json!({ "source": "outside", "name": "Glass", "priceCents": 3_000, "costCents": 1_000 })
        };
        let (status, created) = send(
            app,
            "POST",
            "/api/repairs",
            Some(json!({
                "customer": { "customerKey": self.customer },
                "deviceModel": "Galaxy A55",
                "serialNumber": format!("DEV{}", &uuid::Uuid::new_v4().simple().to_string()[..10]),
                "issueDescription": "Cracked screen",
                "status": "ready",
                "items": [{ "description": "Replace screen", "laborCents": 2_500,
                            "warranty": months(3), "part": part }],
            })),
            &self.staff,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        text(&created["data"]["key"])
    }

    async fn print_job(&self, app: &common::TestApp) -> String {
        let (status, created) = send(
            app,
            "POST",
            "/api/print-jobs",
            Some(json!({
                "customer": { "customerKey": self.customer },
                "jobType": "t-shirt", "quantity": 12, "items": [{ "description": "Printing", "quantity": 1,
                    "source": { "type": "service", "unitPriceCents": 240_000 },
                    "warranty": { "coverage": { "kind": "none" } } }],
            })),
            &self.staff,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        text(&created["data"]["key"])
    }

    async fn get(&self, app: &common::TestApp, path: &str, key: &str) -> Value {
        let (status, body) =
            send(app, "GET", &format!("/api/{path}/{key}"), None, &self.staff).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"].clone()
    }

    async fn bill(&self, app: &common::TestApp, lines: Value, credit: bool) -> (StatusCode, Value) {
        send(
            app,
            "POST",
            "/api/billing/sales",
            Some(sale(&self.customer, lines, credit)),
            &self.staff,
        )
        .await
    }

    async fn stock(&self, app: &common::TestApp) -> i64 {
        let (status, body) = send(
            app,
            "GET",
            &format!("/api/inventory/products/{}", self.screen.1),
            None,
            &self.staff,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"]["stockQuantity"].as_i64().expect("a number")
    }

    async fn warranties_of(&self, app: &common::TestApp, repair_key: &str) -> Vec<Value> {
        let (status, body) = send(
            app,
            "GET",
            &format!("/api/warranties?repairKey={repair_key}"),
            None,
            &self.staff,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"]["items"].as_array().expect("an array").clone()
    }

    async fn patch(
        &self,
        app: &common::TestApp,
        path: &str,
        key: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        send(
            app,
            "PATCH",
            &format!("/api/{path}/{key}"),
            Some(body),
            &self.staff,
        )
        .await
    }
}

fn no_warnings(sold: &Value) {
    assert!(
        sold["data"]["warnings"]
            .as_array()
            .is_none_or(|w| w.is_empty()),
        "{sold}"
    );
}

async fn refusals(app: &common::TestApp, shop: &Shop) {
    let repair = shop.repair(app, true).await;
    let (status, body) = shop
        .bill(app, json!([line("repair", &repair, None)]), false)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "HANDOVER_REQUIRED");

    let adhoc = json!([{ "name": "Cable", "unitPriceCents": 500, "quantity": 1,
                         "discountCents": 0, "sourceType": "retail", "handover": "now" }]);
    let (status, body) = shop.bill(app, adhoc, false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "HANDOVER_NOT_ALLOWED");

    let twice = json!([
        line("repair", &repair, Some("now")),
        line("repair", &repair, Some("later"))
    ]);
    let (status, body) = shop.bill(app, twice, false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "TICKET_LISTED_TWICE");

    let job = shop.get(app, "repairs", &repair).await;
    assert!(
        job.get("billedInvoiceKey").is_none(),
        "nothing was written: {job}"
    );
    assert_eq!(shop.stock(app).await, 6, "nothing left stock");
}

async fn later_then_hand_back(app: &common::TestApp, shop: &Shop) {
    let repair = shop.repair(app, true).await;
    let (status, sold) = shop
        .bill(app, json!([line("repair", &repair, Some("later"))]), true)
        .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    no_warnings(&sold);
    let invoice = &sold["data"]["invoice"];

    let job = shop.get(app, "repairs", &repair).await;
    assert_eq!(
        job["status"], "ready",
        "billing with the hand-over later keeps the status"
    );
    assert_eq!(job["billedInvoiceKey"], invoice["key"]);
    assert_eq!(job["billedInvoiceNumber"], invoice["invoiceNumber"]);
    assert_eq!(
        shop.stock(app).await,
        5,
        "the part left stock with the invoice"
    );
    assert!(
        shop.warranties_of(app, &repair).await.is_empty(),
        "no warranty until the job is handed back"
    );

    // Billed: not billed twice, work locked.
    let (status, body) = shop
        .bill(app, json!([line("repair", &repair, Some("now"))]), false)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "TICKET_ALREADY_BILLED");
    let (status, body) = shop
        .patch(app, "repairs", &repair, json!({ "items": [] }))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "REPAIR_ITEMS_LOCKED");

    // Handed back by hand: allowed because it is billed; the warranty starts today.
    let (status, body) = shop
        .patch(app, "repairs", &repair, json!({ "status": "delivered" }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "delivered");
    let issued = shop.warranties_of(app, &repair).await;
    assert_eq!(issued.len(), 1, "{issued:?}");
    assert_eq!(issued[0]["invoiceKey"], invoice["key"]);
    assert_eq!(issued[0]["invoiceNumber"], invoice["invoiceNumber"]);
    assert_eq!(issued[0]["startsOn"], shop_today().as_str());

    // Reopened and handed back again: no second set of warranties.
    let (status, body) = shop
        .patch(app, "repairs", &repair, json!({ "status": "ready" }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = shop
        .patch(app, "repairs", &repair, json!({ "status": "delivered" }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(shop.warranties_of(app, &repair).await.len(), 1);
}

async fn now_delivers_at_once(app: &common::TestApp, shop: &Shop) {
    let repair = shop.repair(app, true).await;
    let before = shop.stock(app).await;
    let (status, sold) = shop
        .bill(app, json!([line("repair", &repair, Some("now"))]), false)
        .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    no_warnings(&sold);
    let job = shop.get(app, "repairs", &repair).await;
    assert_eq!(job["status"], "delivered");
    assert_eq!(job["billedInvoiceKey"], sold["data"]["invoice"]["key"]);
    assert_eq!(shop.stock(app).await, before - 1);
    let issued = shop.warranties_of(app, &repair).await;
    assert_eq!(issued.len(), 1);
    assert_eq!(issued[0]["startsOn"], shop_today().as_str());
}

async fn hand_back_rules(app: &common::TestApp, shop: &Shop) {
    // Unbilled with a stock part: still only by billing it.
    let stocked = shop.repair(app, true).await;
    let (status, body) = shop
        .patch(app, "repairs", &stocked, json!({ "status": "delivered" }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "REPAIR_PART_INVALID");

    // Handed back by hand with no stock part: cannot then be billed.
    let outside = shop.repair(app, false).await;
    let (status, body) = shop
        .patch(app, "repairs", &outside, json!({ "status": "delivered" }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = shop
        .bill(app, json!([line("repair", &outside, Some("now"))]), false)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "INVALID_JOB_STATUS");
}

async fn voiding_removes_the_mark(app: &common::TestApp, shop: &Shop) {
    let repair = shop.repair(app, false).await;
    let (status, sold) = shop
        .bill(app, json!([line("repair", &repair, Some("later"))]), false)
        .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    let invoice_key = text(&sold["data"]["invoice"]["key"]);
    let (status, body) = send(
        app,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/void"),
        Some(json!({ "reason": "Entered by mistake" })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let job = shop.get(app, "repairs", &repair).await;
    assert!(job.get("billedInvoiceKey").is_none(), "{job}");
    assert!(job.get("billedInvoiceNumber").is_none(), "{job}");
    assert_eq!(job["status"], "ready", "the status is not reverted");

    // It can be billed again.
    let (status, sold) = shop
        .bill(app, json!([line("repair", &repair, Some("later"))]), false)
        .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    let job = shop.get(app, "repairs", &repair).await;
    assert_eq!(job["billedInvoiceKey"], sold["data"]["invoice"]["key"]);
}

async fn print_jobs(app: &common::TestApp, shop: &Shop) {
    let later = shop.print_job(app).await;
    let (status, sold) = shop
        .bill(app, json!([line("print", &later, Some("later"))]), true)
        .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    no_warnings(&sold);
    let job = shop.get(app, "print-jobs", &later).await;
    assert_eq!(job["status"], "received", "the status is kept");
    assert_eq!(job["billedInvoiceKey"], sold["data"]["invoice"]["key"]);
    let (status, body) = shop
        .bill(app, json!([line("print", &later, Some("now"))]), false)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "TICKET_ALREADY_BILLED");
    let (status, body) = shop
        .patch(app, "print-jobs", &later, json!({ "status": "delivered" }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let now = shop.print_job(app).await;
    let (status, sold) = shop
        .bill(app, json!([line("print", &now, Some("now"))]), false)
        .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    let job = shop.get(app, "print-jobs", &now).await;
    assert_eq!(job["status"], "delivered");
    assert_eq!(job["billedInvoiceKey"], sold["data"]["invoice"]["key"]);

    // Delivered by hand, it cannot be billed.
    let by_hand = shop.print_job(app).await;
    let (status, body) = shop
        .patch(
            app,
            "print-jobs",
            &by_hand,
            json!({ "status": "delivered" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = shop
        .bill(app, json!([line("print", &by_hand, Some("now"))]), false)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "INVALID_JOB_STATUS");

    // Voiding removes the mark.
    let invoice_key = text(&sold["data"]["invoice"]["key"]);
    let (status, body) = send(
        app,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/void"),
        Some(json!({ "reason": "Entered by mistake" })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let job = shop.get(app, "print-jobs", &now).await;
    assert!(job.get("billedInvoiceKey").is_none(), "{job}");
}

/// A sale of `lines` to `buyer`; `None` is a walk-in.
fn sale_to(buyer: Option<&str>, lines: Value, credit: bool) -> Value {
    let mut body = sale("", lines, credit);
    match buyer {
        Some(key) => body["customer"] = json!({ "customerKey": key }),
        None => {
            body.as_object_mut().expect("a sale").remove("customer");
        }
    }
    body
}

/// A ticket line that says the cashier confirmed billing `owner`'s job to someone else.
fn confirmed(ticket_type: &str, ticket_key: &str, handover: &str, owner: &str) -> Value {
    let mut line = line(ticket_type, ticket_key, Some(handover));
    line["confirmedJobCustomerKey"] = json!(owner);
    line
}

async fn another_customers_job(app: &common::TestApp, shop: &Shop) {
    let payer = common::register_customer(app, "Kamal Silva", "0779999999").await;
    let sell = |buyer: Option<String>, lines: Value, credit: bool| async move {
        send(
            app,
            "POST",
            "/api/billing/sales",
            Some(sale_to(buyer.as_deref(), lines, credit)),
            &shop.staff,
        )
        .await
    };

    let repair = shop.repair(app, true).await;
    let before = shop.stock(app).await;
    // Someone else, or a walk-in, without a confirmation or with the wrong one: nothing happens.
    for (buyer, lines) in [
        (
            Some(payer.clone()),
            json!([line("repair", &repair, Some("now"))]),
        ),
        (None, json!([line("repair", &repair, Some("now"))])),
        (
            Some(payer.clone()),
            json!([confirmed("repair", &repair, "now", &payer)]),
        ),
    ] {
        let (status, body) = sell(buyer, lines, false).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["code"], "TICKET_OTHER_CUSTOMER");
        assert!(
            body["message"]
                .as_str()
                .expect("a message")
                .contains("Nimal Perera"),
            "{body}"
        );
    }
    // A confirmation nothing asks for is refused: on the owner's own sale and on another line.
    let retail = json!([{ "name": "Cable", "unitPriceCents": 500, "quantity": 1,
        "discountCents": 0, "sourceType": "retail", "confirmedJobCustomerKey": shop.customer }]);
    for lines in [
        json!([confirmed("repair", &repair, "now", &shop.customer)]),
        retail,
    ] {
        let (status, body) = sell(Some(shop.customer.clone()), lines, false).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["code"], "JOB_CUSTOMER_CONFIRMATION_NOT_ALLOWED");
    }
    let job = shop.get(app, "repairs", &repair).await;
    assert!(job.get("billedInvoiceKey").is_none(), "{job}");
    assert_eq!(shop.stock(app).await, before, "nothing left stock");

    // Confirmed: the payer's invoice, the job owner's warranty.
    let (status, sold) = sell(
        Some(payer.clone()),
        json!([confirmed("repair", &repair, "now", &shop.customer)]),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    no_warnings(&sold);
    let invoice = &sold["data"]["invoice"];
    assert_eq!(invoice["customerKey"], payer.as_str());
    let issued = shop.warranties_of(app, &repair).await;
    assert_eq!(issued.len(), 1, "{issued:?}");
    assert_eq!(issued[0]["customerKey"], shop.customer.as_str());
    assert_eq!(issued[0]["customerName"], "Nimal Perera");
    assert_eq!(issued[0]["customerPhone"], "0771234567");

    // The owner's claim can still be refunded from the payer's invoice.
    let (status, claim) = send(
        app,
        "POST",
        "/api/warranty-claims",
        Some(json!({ "warrantyKey": issued[0]["key"], "issueDescription": "Cracked again" })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claim}");
    assert_eq!(claim["data"]["customerKey"], shop.customer.as_str());
    let claim_key = text(&claim["data"]["key"]);
    for (step, body) in [
        ("inspect", json!({ "inspectionNotes": "Glass lifted" })),
        ("decide", json!({ "decision": "refund" })),
    ] {
        let (status, answer) = send(
            app,
            "POST",
            &format!("/api/warranty-claims/{claim_key}/{step}"),
            Some(body),
            &shop.admin,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{step}: {answer}");
    }
    let (status, note) = send(
        app,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice["key"],
            "warrantyClaimKey": claim_key,
            "returnedItems": [{
                "sourceTicketKey": repair, "name": invoice["items"][0]["name"],
                "quantity": 1, "reason": "defective", "condition": "resalable",
                "serialNumber": issued[0]["serialNumber"],
            }],
            "paymentMethod": "cash",
        })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{note}");

    // Handed back later, by a walk-in: still the job owner's warranty.
    let later = shop.repair(app, false).await;
    let (status, sold) = sell(
        None,
        json!([confirmed("repair", &later, "later", &shop.customer)]),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    let (status, body) = shop
        .patch(app, "repairs", &later, json!({ "status": "delivered" }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let issued = shop.warranties_of(app, &later).await;
    assert_eq!(issued[0]["customerKey"], shop.customer.as_str());

    // Print jobs follow the same two paths.
    for handover in ["now", "later"] {
        let (status, created) = send(
            app,
            "POST",
            "/api/print-jobs",
            Some(json!({
                "customer": { "customerKey": shop.customer },
                "jobType": "mug", "quantity": 2,
                "items": [{ "description": "Printing", "quantity": 2,
                    "source": { "type": "service", "unitPriceCents": 40_000 },
                    "warranty": months(1) }],
            })),
            &shop.staff,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let print = text(&created["data"]["key"]);
        let (status, body) = sell(
            Some(payer.clone()),
            json!([line("print", &print, Some(handover))]),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["code"], "TICKET_OTHER_CUSTOMER");
        let (status, sold) = sell(
            Some(payer.clone()),
            json!([confirmed("print", &print, handover, &shop.customer)]),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{sold}");
        no_warnings(&sold);
        if handover == "later" {
            let (status, body) = shop
                .patch(app, "print-jobs", &print, json!({ "status": "delivered" }))
                .await;
            assert_eq!(status, StatusCode::OK, "{body}");
        }
        let (status, found) = send(
            app,
            "GET",
            &format!("/api/warranties?printJobKey={print}"),
            None,
            &shop.staff,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert_eq!(
            found["data"]["items"][0]["customerKey"],
            shop.customer.as_str(),
            "{handover}: {found}"
        );
    }
}

/// The marks of tickets billed before the mark existed are worked out from their invoices.
async fn backfill(app: &common::TestApp, shop: &Shop) {
    let marked = shop.repair(app, false).await;
    let (status, first) = shop
        .bill(app, json!([line("repair", &marked, Some("later"))]), false)
        .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let voided = shop.repair(app, false).await;
    let (status, sold) = shop
        .bill(app, json!([line("repair", &voided, Some("later"))]), false)
        .await;
    assert_eq!(status, StatusCode::OK, "{sold}");
    let (status, body) = send(
        app,
        "POST",
        &format!(
            "/api/billing/invoices/{}/void",
            text(&sold["data"]["invoice"]["key"])
        ),
        Some(json!({ "reason": "Entered by mistake" })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let never = shop.repair(app, false).await;

    // Forget every mark, as a database from before this change would have none.
    match app.db_handle.as_sqlite() {
        Some(pool) => {
            for table in ["repairs", "print_jobs"] {
                sqlx::query(&format!(
                    "UPDATE {table} SET billed_invoice_key = NULL, billed_invoice_number = NULL"
                ))
                .execute(pool)
                .await
                .expect("the marks are forgotten");
            }
        }
        None => {
            for table in ["repairs", "print_jobs"] {
                app.mongo()
                    .collection::<Document>(table)
                    .update_many(
                        doc! {},
                        doc! { "$unset": { "billed_invoice_key": "", "billed_invoice_number": "" } },
                    )
                    .await
                    .expect("the marks are forgotten");
            }
        }
    }
    let run = || async {
        match &app.db_handle {
            Db::Sqlite(pool) => migrate_sqlite(pool)
                .await
                .expect("the SQLite migration runs"),
            Db::Mongo(_) => {
                migrate_mongo(app.mongo(), Mode::Apply)
                    .await
                    .expect("the Mongo migration runs");
            }
        }
    };
    run().await;
    let job = shop.get(app, "repairs", &marked).await;
    assert_eq!(job["billedInvoiceKey"], first["data"]["invoice"]["key"]);
    assert_eq!(
        job["billedInvoiceNumber"],
        first["data"]["invoice"]["invoiceNumber"]
    );
    for key in [&voided, &never] {
        let job = shop.get(app, "repairs", key).await;
        assert!(job.get("billedInvoiceKey").is_none(), "{job}");
    }
    // Running it again changes nothing.
    let version = job["version"].clone();
    run().await;
    assert_eq!(shop.get(app, "repairs", &marked).await["version"], version);
}

async fn scenario(app: &common::TestApp) {
    let shop = Shop::open(app).await;
    refusals(app, &shop).await;
    later_then_hand_back(app, &shop).await;
    now_delivers_at_once(app, &shop).await;
    hand_back_rules(app, &shop).await;
    voiding_removes_the_mark(app, &shop).await;
    print_jobs(app, &shop).await;
    another_customers_job(app, &shop).await;
    backfill(app, &shop).await;
}

#[tokio::test]
async fn ticket_handover_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn ticket_handover_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}
