//! Invoices and stats tests.

use axum::body::Body;
use axum::http::header::AUTHORIZATION;
use axum::http::{Request, StatusCode};

use mongodb::bson::DateTime as BsonDateTime;
use serde_json::{Value, json};

use simplebash_pos_backend::core::id::generate_id;
use simplebash_pos_backend::domain::billing::InvoiceStatus;

use simplebash_pos_backend::modules::billing::model::InvoiceDocument;

use crate::common;
use crate::helpers::{base_sale_payload, send_anon, send_authed, staff_token};
use tower::ServiceExt;
use uuid::Uuid;

/// Inserts a minimal `InvoiceDocument` directly, bypassing `complete_sale`,
/// with full control over the fields the list-filter tests need. Returns
/// `(key, invoiceNumber)` — `invoiceNumber` is uuid-suffixed and unique per
/// call, so it doubles as a search term to scope a query to just this row
/// even against the shared test DB.
#[allow(clippy::too_many_arguments)]
async fn seed_invoice_full(
    db: &mongodb::Database,
    total_cents: i64,
    is_credit: bool,
    status: &str,
    payment_method: &str,
    customer_name: Option<&str>,
    created_at: BsonDateTime,
) -> (String, String) {
    let key = generate_id("inv");
    let invoice_number = format!("INV-{}", Uuid::new_v4().simple());
    db.collection::<InvoiceDocument>("invoices")
        .insert_one(InvoiceDocument {
            id: None,
            key: key.clone(),
            invoice_number: invoice_number.clone(),
            customer_key: None,
            customer_name_snapshot: customer_name.map(|s| s.to_string()),
            customer_phone_snapshot: None,
            customer_address_snapshot: None,
            cashier_id: "staff-1".to_string(),
            cashier_name_snapshot: "Test Cashier".to_string(),
            items: vec![],
            subtotal_cents: total_cents,
            discount_type: "fixed".to_string(),
            discount_value: 0.0,
            discount_cents: 0,
            total_cents,
            payment_method: payment_method.to_string(),
            split_payments: None,
            is_credit,
            amount_received_cents: None,
            change_due_cents: None,
            due_date: None,
            card_last4: None,
            card_ref: None,
            online_ref: None,
            online_note: None,
            status: match status {
                "pending" => InvoiceStatus::Pending,
                "partially_paid" => InvoiceStatus::PartiallyPaid,
                "paid" => InvoiceStatus::Paid,
                "voided" => InvoiceStatus::Voided,
                "closed" => InvoiceStatus::Closed,
                other => panic!("seed_invoice_full: unknown status '{other}'"),
            },
            notes: None,
            shop_profile_snapshot: json!({}),
            warranty_terms_snapshot: None,
            document_selection: None,
            voided_at: None,
            voided_by: None,
            voided_reason: None,
            closed_at: None,
            closed_by: None,
            credit_note_count: 0,
            refunded_cents: 0,
            session_key: None,
            version: 1,
            created_at,
            updated_at: created_at,
            updated_by_device: None,
        })
        .await
        .expect("the awaited step succeeds");
    (key, invoice_number)
}

async fn seed_invoice_with(
    db: &mongodb::Database,
    total_cents: i64,
    is_credit: bool,
    status: &str,
    created_at: BsonDateTime,
) -> String {
    let payment_method = if is_credit { "credit" } else { "cash" };
    let (key, _) = seed_invoice_full(
        db,
        total_cents,
        is_credit,
        status,
        payment_method,
        None,
        created_at,
    )
    .await;
    key
}

async fn seed_invoice_with_number(
    db: &mongodb::Database,
    invoice_number: &str,
    customer_name: Option<&str>,
) -> String {
    let key = generate_id("inv");
    let now = BsonDateTime::now();
    db.collection::<InvoiceDocument>("invoices")
        .insert_one(InvoiceDocument {
            id: None,
            key: key.clone(),
            invoice_number: invoice_number.to_string(),
            customer_key: None,
            customer_name_snapshot: customer_name.map(|s| s.to_string()),
            customer_phone_snapshot: None,
            customer_address_snapshot: None,
            cashier_id: "staff-1".to_string(),
            cashier_name_snapshot: "Test Cashier".to_string(),
            items: vec![],
            subtotal_cents: 1000,
            discount_type: "fixed".to_string(),
            discount_value: 0.0,
            discount_cents: 0,
            total_cents: 1000,
            payment_method: "cash".to_string(),
            split_payments: None,
            is_credit: false,
            amount_received_cents: None,
            change_due_cents: None,
            due_date: None,
            card_last4: None,
            card_ref: None,
            online_ref: None,
            online_note: None,
            status: InvoiceStatus::Paid,
            notes: None,
            shop_profile_snapshot: json!({}),
            warranty_terms_snapshot: None,
            document_selection: None,
            voided_at: None,
            voided_by: None,
            voided_reason: None,
            closed_at: None,
            closed_by: None,
            credit_note_count: 0,
            refunded_cents: 0,
            session_key: None,
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        })
        .await
        .expect("the awaited step succeeds");
    key
}

#[tokio::test]
async fn billing_stats_endpoint_requires_auth() {
    let app = common::spawn_app().await;
    let (status, _) = send_anon(&app.router, "GET", "/api/billing/invoices/stats", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn billing_stats_endpoint_returns_today_sales_invoice_count_outstanding_credit_and_avg_basket()
 {
    // `spawn_app`'s DB (`MONGODB_TEST_DB_NAME`) is shared across the whole
    // suite, which every other test tolerates by scoping its own queries
    // (a unique search substring, a specific key) — not possible here since
    // the stats endpoint aggregates the WHOLE collection with no filter.
    // So this test connects its own throwaway database, seeds only into
    // that, and calls the service function directly (bypassing HTTP/router
    // entirely) to get exact, uncontaminated numbers.
    // Atlas caps database names at 38 bytes.
    let db_name = format!("jtstats_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let db = simplebash_pos_backend::clients::mongo::connect(&common::mongo_uri(), &db_name)
        .await
        .expect("failed to connect to isolated stats test database");

    let now = BsonDateTime::now();
    let yesterday = BsonDateTime::from_chrono(now.to_chrono() - chrono::Duration::days(1));

    // Today: one paid cash sale (2000), one paid credit sale (500 — fully
    // paid, so it does NOT count toward outstanding credit).
    seed_invoice_with(&db, 2000, false, "paid", now).await;
    seed_invoice_with(&db, 500, true, "paid", now).await;
    // Today but pending (not credit) — counts toward outstanding.
    seed_invoice_with(&db, 300, false, "pending", now).await;
    // Backdated (outside "today") paid invoice — must NOT count toward
    // today's sales/count, but a backdated pending one still counts toward
    // the all-time outstanding-credit sum.
    seed_invoice_with(&db, 10_000, false, "paid", yesterday).await;
    seed_invoice_with(&db, 700, false, "pending", yesterday).await;

    let db_handle = simplebash_pos_backend::clients::db::Db::from_mongo(db.clone());
    let stats = simplebash_pos_backend::modules::billing::service::get_billing_stats(&db_handle)
        .await
        .expect("get_billing_stats should succeed");

    // today_sales/count only include the 3 invoices created "now".
    assert_eq!(stats.today_sales_cents, 2000 + 500 + 300);
    assert_eq!(stats.today_invoice_count, 3);
    // outstanding credit = remaining balance on every pending/partially-paid
    // invoice, any date: 300 (today, pending) + 700 (yesterday, pending).
    // The fully-paid credit sale (500) contributes nothing.
    assert_eq!(stats.outstanding_credit_cents, 300 + 700);
    // avg basket = today's sales / today's count, rounded.
    assert_eq!(stats.avg_basket_cents, (2000 + 500 + 300) / 3);

    db.drop().await.ok();
}

#[tokio::test]
async fn list_invoices_filters_by_payment_status_and_payment_method() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let unique = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let name = format!("PayFilter-{unique}");
    let now = BsonDateTime::now();

    // Paid cash sale.
    let (_, paid_number) =
        seed_invoice_full(app.mongo(), 1000, false, "paid", "cash", Some(&name), now).await;
    // Paid card sale — same customer name, different payment method.
    let (_, card_number) =
        seed_invoice_full(app.mongo(), 1500, false, "paid", "card", Some(&name), now).await;
    // Credit sale (is_credit true, still "paid" status) — same customer name.
    let (_, credit_number) =
        seed_invoice_full(app.mongo(), 2000, true, "paid", "cash", Some(&name), now).await;

    // paymentStatus=paid excludes the credit sale.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices?search={name}&paymentStatus=paid"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let numbers: Vec<String> = body["data"]["invoices"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|inv| {
            inv["invoiceNumber"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        })
        .collect();
    assert!(numbers.contains(&paid_number));
    assert!(numbers.contains(&card_number));
    assert!(!numbers.contains(&credit_number));

    // paymentStatus=credit returns only the credit sale.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices?search={name}&paymentStatus=credit"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let numbers: Vec<String> = body["data"]["invoices"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|inv| {
            inv["invoiceNumber"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        })
        .collect();
    assert_eq!(numbers, vec![credit_number]);

    // paymentMethod=card returns only the card sale.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices?search={name}&paymentMethod=card"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let numbers: Vec<String> = body["data"]["invoices"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|inv| {
            inv["invoiceNumber"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        })
        .collect();
    assert_eq!(numbers, vec![card_number]);
}

#[tokio::test]
async fn list_invoices_filters_by_date_preset() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let unique = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let name = format!("DatePreset-{unique}");
    let now = BsonDateTime::now();
    let yesterday = BsonDateTime::from_chrono(now.to_chrono() - chrono::Duration::days(1));
    let (_, today_number) =
        seed_invoice_full(app.mongo(), 1000, false, "paid", "cash", Some(&name), now).await;
    seed_invoice_full(
        app.mongo(),
        2000,
        false,
        "paid",
        "cash",
        Some(&name),
        yesterday,
    )
    .await;

    // No datePreset — search alone finds both.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices?search={name}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(
        body["data"]["invoices"]
            .as_array()
            .expect("the value is an array")
            .len(),
        2
    );

    // datePreset=today narrows to just the one created "now".
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices?search={name}&datePreset=today"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let invoices = body["data"]["invoices"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(invoices.len(), 1, "Body: {body}");
    assert_eq!(invoices[0]["invoiceNumber"], today_number);
}

#[tokio::test]
async fn list_invoices_searches_by_sequence_number() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    seed_invoice_with_number(app.mongo(), "INV-000001", Some("Sequence Customer")).await;

    for search_term in ["000001", "1", "00001", "INV-1", "inv-000001"] {
        let (status, body) = send_authed(
            &app.router,
            "GET",
            &format!("/api/billing/invoices?search={search_term}"),
            None,
            &token,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "Failed for search={search_term}: {body}"
        );
        let invoices = body["data"]["invoices"]
            .as_array()
            .expect("the value is an array");
        let found = invoices
            .iter()
            .any(|inv| inv["invoiceNumber"] == "INV-000001");
        assert!(
            found,
            "Expected to find INV-000001 when searching for '{search_term}', found: {invoices:?}"
        );
    }
}

// The integration contract: the POS side durably records the
// document-server-minted template key (`tpl_...`) on every generated
// document row — not just the human-readable layout name.
#[tokio::test]
async fn get_invoice_document_persists_document_server_template_key() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "name": "Widget",
        "unitPriceCents": 1000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (_, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/invoices/{invoice_key}/documents/a4-invoice?design=current"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("the request is well formed");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    assert_eq!(response.status(), StatusCode::OK);

    let record = app
        .mongo()
        .collection::<mongodb::bson::Document>("generated_documents")
        .find_one(mongodb::bson::doc! { "entity_key": &invoice_key })
        .await
        .expect("query generated_documents")
        .expect("a rendered document should have been recorded");
    assert_eq!(
        record
            .get_str("template_name")
            .expect("the document has this string field"),
        "a4-invoice",
        "layout name recorded as before"
    );
    let template_key = record.get_str("template_key").expect(
        "template_key must be recorded on new rows — the POS side holding the \
         document-server's unique ID is the point of this contract",
    );
    // The mock document-server publishes exactly this key for the template.
    assert_eq!(template_key, "tpl_a4_invoice");
    // With no design chosen, the built-in default prints under the kind's
    // historical cache key — so PDFs cached before designs were selectable
    // stay valid.
    assert_eq!(
        record
            .get_str("document_type")
            .expect("the document has this string field"),
        "a4-invoice"
    );
}

// Pre-validation: a payload that violates the schema the document-server
// publishes for a template fails HERE with 422 RENDER_VALIDATION_FAILED —
// no wasted round trip, field-level message naming the violated contract.
// Uses its own strict-schema mock so other tests' happy paths are untouched.
#[tokio::test]
async fn document_render_payload_violating_published_schema_fails_fast_locally() {
    let strict_mock_url = common::start_strict_mock_document_server().await;
    let app = common::spawn_app_with_document_server_url(strict_mock_url).await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "name": "Widget",
        "unitPriceCents": 1000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (_, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/invoices/{invoice_key}/documents/a4-invoice?design=current"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("the request is well formed");
    let response = app
        .router
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the awaited step succeeds"),
    )
    .expect("the text is valid JSON");
    assert_eq!(body["code"], "RENDER_VALIDATION_FAILED");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("__never_sent_by_backend"),
        "rejection should come from local pre-validation and name the missing \
         required field, got: {message}"
    );
}
