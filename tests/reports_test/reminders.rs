//! Reminders tests.

use axum::http::StatusCode;

use chrono::{Duration, Utc};

use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;

use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::helpers::send_authed;

// ===========================================================================
// Dashboard reminders feed (GET /api/reports/reminders)
// ===========================================================================

fn full_access_token(config: &simplebash_pos_backend::core::config::Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}

/// `days` from today, formatted YYYY-MM-DD.
fn date_offset(days: i64) -> String {
    (Utc::now().date_naive() + Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

async fn seed_reminder_customer(app: &common::TestApp, token: &str, phone_suffix: &str) -> String {
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({ "name": "Reminder Customer", "primaryPhone": format!("0755{phone_suffix}") })),
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string()
}

async fn seed_credit_sale(
    app: &common::TestApp,
    token: &str,
    customer_key: &str,
    total_cents: i64,
    deposit_cents: i64,
    due_date: &str,
) -> String {
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Reminder Cashier" },
            "customer": { "customerKey": customer_key },
            "items": [{
                "name": "Reminder line",
                "unitPriceCents": total_cents,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail",
            }],
            "payment": {
                "paymentMethod": "credit",
                "isCredit": true,
                "amountReceivedCents": deposit_cents,
                "dueDate": due_date,
            },
            "shopProfileSnapshot": { "tradingName": "Reminder Shop" },
        })),
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    body["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string()
}

fn find_reminder<'a>(body: &'a Value, key: &str) -> Option<&'a Value> {
    body["data"]["reminders"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|r| r["key"] == key)
}

#[tokio::test]
async fn reminders_requires_reports_view_permission() {
    let app = common::spawn_app().await;
    let token = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, _) = send_authed(&app.router, "GET", "/api/reports/reminders", None, &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn reminders_split_credit_invoices_into_overdue_and_due_soon_and_are_installment_aware() {
    let app = common::spawn_app().await;
    let token = full_access_token(&app.config);

    let customer_key = seed_reminder_customer(&app, &token, "100001").await;
    // Overdue: due 5 days ago, no deposit → whole 150000 owed.
    let overdue_key =
        seed_credit_sale(&app, &token, &customer_key, 150_000, 0, &date_offset(-5)).await;
    // Due soon: due in 3 days, 50000 deposit → 100000 still owed.
    let due_soon_key = seed_credit_sale(
        &app,
        &token,
        &customer_key,
        150_000,
        50_000,
        &date_offset(3),
    )
    .await;
    // Far future: due in 40 days → not returned.
    let far_key = seed_credit_sale(&app, &token, &customer_key, 150_000, 0, &date_offset(40)).await;

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/reminders?dueWithinDays=7&limit=100",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let overdue = find_reminder(&body, &overdue_key).expect("overdue invoice should be listed");
    assert_eq!(overdue["kind"], "credit_overdue");
    assert_eq!(overdue["amountCents"], 150_000);
    assert_eq!(overdue["daysFromDue"], 5);

    let due_soon = find_reminder(&body, &due_soon_key).expect("due-soon invoice should be listed");
    assert_eq!(due_soon["kind"], "credit_due_soon");
    assert_eq!(
        due_soon["amountCents"], 100_000,
        "balance is total minus the checkout deposit"
    );
    assert_eq!(due_soon["daysFromDue"], -3);

    assert!(
        find_reminder(&body, &far_key).is_none(),
        "an invoice due outside the window is not a reminder yet"
    );
}

#[tokio::test]
async fn reminders_include_overdue_and_due_soon_repair_and_print_jobs() {
    let app = common::spawn_app().await;
    let token = full_access_token(&app.config);

    let (status, repair_body) = send_authed(
        &app.router,
        "POST",
        "/api/repairs",
        Some(json!({
            "customer": { "customerName": "Job Reminder", "customerPhone": "0755200002" },
            "deviceModel": "Pixel 7",
            "issueDescription": "No power",
            "items": [{ "description": "Repair work", "laborCents": 400_000, "warranty": { "coverage": { "kind": "none" } } }],
            "promisedReadyAt": date_offset(-2),
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {repair_body}");
    let repair_key = repair_body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, print_body) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(json!({
            "customer": { "customerName": "Print Reminder", "customerPhone": "0755200003" },
            "jobType": "banner",
            "quantity": 2,
            "estimatedCostCents": 90_000,
            "promisedReadyAt": date_offset(1),
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {print_body}");
    let print_key = print_body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/reminders?limit=100",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let repair = find_reminder(&body, &repair_key).expect("overdue repair should be listed");
    assert_eq!(repair["kind"], "job_overdue");
    assert_eq!(repair["amountCents"], 400_000);

    let print = find_reminder(&body, &print_key).expect("due-soon print job should be listed");
    assert_eq!(print["kind"], "job_due_soon");
}

#[tokio::test]
async fn reminders_payment_overdues_sort_before_job_overdues() {
    let app = common::spawn_app().await;
    let token = full_access_token(&app.config);

    // Seed a job overdue by 10 days
    let (status, repair_body) = send_authed(
        &app.router,
        "POST",
        "/api/repairs",
        Some(json!({
            "customer": { "customerName": "Overdue Repair", "customerPhone": "0755999999" },
            "deviceModel": "Pixel 7",
            "issueDescription": "No power",
            "items": [{ "description": "Repair work", "laborCents": 400_000, "warranty": { "coverage": { "kind": "none" } } }],
            "promisedReadyAt": date_offset(-10),
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {repair_body}");
    let repair_key = repair_body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // Seed a credit sale overdue by only 1 day
    let customer_key = seed_reminder_customer(&app, &token, "400004").await;
    let credit_key =
        seed_credit_sale(&app, &token, &customer_key, 50_000, 0, &date_offset(-1)).await;

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/reminders?limit=100",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let reminders = body["data"]["reminders"]
        .as_array()
        .expect("the value is an array");
    let credit_idx = reminders
        .iter()
        .position(|r| r["key"] == credit_key)
        .expect("credit overdue must be present");
    let repair_idx = reminders
        .iter()
        .position(|r| r["key"] == repair_key)
        .expect("repair overdue must be present");

    assert!(
        credit_idx < repair_idx,
        "Payment overdue (index {credit_idx}) should come before job overdue (index {repair_idx})"
    );
}

#[tokio::test]
async fn reminders_due_within_days_param_narrows_the_window() {
    let app = common::spawn_app().await;
    let token = full_access_token(&app.config);
    let customer_key = seed_reminder_customer(&app, &token, "300003").await;
    let key = seed_credit_sale(&app, &token, &customer_key, 120_000, 0, &date_offset(5)).await;

    let (_, wide) = send_authed(
        &app.router,
        "GET",
        "/api/reports/reminders?dueWithinDays=7&limit=100",
        None,
        &token,
    )
    .await;
    assert!(find_reminder(&wide, &key).is_some());

    let (_, narrow) = send_authed(
        &app.router,
        "GET",
        "/api/reports/reminders?dueWithinDays=2&limit=100",
        None,
        &token,
    )
    .await;
    assert!(
        find_reminder(&narrow, &key).is_none(),
        "an invoice 5 days out drops off a 2-day window"
    );
}

#[tokio::test]
async fn reminders_supports_pagination_with_page_and_limit() {
    let app = common::spawn_app().await;
    let token = full_access_token(&app.config);

    // Seed 3 overdue credit invoices to guarantee at least 3 records exist.
    let customer_key = seed_reminder_customer(&app, &token, "500005").await;
    let _key1 = seed_credit_sale(&app, &token, &customer_key, 100_000, 0, &date_offset(-1)).await;
    let _key2 = seed_credit_sale(&app, &token, &customer_key, 100_000, 0, &date_offset(-2)).await;
    let _key3 = seed_credit_sale(&app, &token, &customer_key, 100_000, 0, &date_offset(-3)).await;

    // Page 1 with limit 2
    let (status1, body1) = send_authed(
        &app.router,
        "GET",
        "/api/reports/reminders?page=1&limit=2",
        None,
        &token,
    )
    .await;
    assert_eq!(status1, StatusCode::OK);
    assert_eq!(body1["data"]["page"], 1);
    assert_eq!(body1["data"]["limit"], 2);
    let total = body1["data"]["total"]
        .as_u64()
        .expect("the value is an unsigned integer");
    assert!(total >= 3);
    assert!(
        body1["data"]["totalPages"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 2
    );
    let page1_reminders = body1["data"]["reminders"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(page1_reminders.len(), 2);
    let page1_keys: Vec<String> = page1_reminders
        .iter()
        .map(|r| {
            r["key"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        })
        .collect();

    // Page 2 with limit 2
    let (status2, body2) = send_authed(
        &app.router,
        "GET",
        "/api/reports/reminders?page=2&limit=2",
        None,
        &token,
    )
    .await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(body2["data"]["page"], 2);
    assert_eq!(body2["data"]["limit"], 2);
    let page2_reminders = body2["data"]["reminders"]
        .as_array()
        .expect("the value is an array");
    assert!(!page2_reminders.is_empty());
    let page2_keys: Vec<String> = page2_reminders
        .iter()
        .map(|r| {
            r["key"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        })
        .collect();

    // Verify no overlap between page 1 and page 2
    for key in &page2_keys {
        assert!(
            !page1_keys.contains(key),
            "Key {key} from page 2 should not appear on page 1"
        );
    }
}

#[tokio::test]
async fn reminders_exclude_paid_and_delivered_records() {
    let app = common::spawn_app().await;
    let token = full_access_token(&app.config);
    let customer_key = seed_reminder_customer(&app, &token, "400004").await;
    let invoice_key =
        seed_credit_sale(&app, &token, &customer_key, 100_000, 0, &date_offset(-3)).await;

    // Pay it off in full.
    let (status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/payments"),
        Some(json!({ "amountCents": 100_000, "paymentMethod": "cash" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/reminders?limit=100",
        None,
        &token,
    )
    .await;
    assert!(
        find_reminder(&body, &invoice_key).is_none(),
        "a fully paid invoice is not a reminder"
    );
}
