mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use chrono::{Duration, Utc};
use simplebash_pos_backend::{
    core::{constants::roles, id::generate_id},
    domain::{
        billing::{InvoiceItem, InvoiceStatus},
        reports::TimeSeriesPoint,
        users::Role,
    },
    modules::{billing::model::InvoiceDocument, reports::engine::charts::enrich_timeseries},
};
use mongodb::bson::DateTime as BsonDateTime;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn execute(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

async fn send_authed(
    router: &axum::Router,
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
        .unwrap();
    execute(router, request).await
}

fn seed_invoice(
    key: &str,
    created_at: chrono::DateTime<Utc>,
    line_total_cents: i64,
    unit_cost_cents: i64,
) -> InvoiceDocument {
    InvoiceDocument {
        id: None,
        key: key.to_string(),
        invoice_number: format!("INV-{}", generate_id("t")),
        customer_key: None,
        customer_name_snapshot: Some("Walk-in".to_string()),
        customer_phone_snapshot: None,
        customer_address_snapshot: None,
        cashier_id: "usr_cashier_1".to_string(),
        cashier_name_snapshot: "Alice Cashier".to_string(),
        items: vec![InvoiceItem {
            product_key: Some(format!("prd_{key}")),
            name: "Engine Test Item".to_string(),
            sku: Some("ENG-TST-0001".to_string()),
            unit_price_cents: line_total_cents,
            quantity: 1,
            discount_cents: 0,
            total_cents: line_total_cents,
            unit_cost_cents: Some(unit_cost_cents),
            source_type: "retail".to_string(),
            source_ticket_key: None,
            source_ticket_number: None,
            assigned_employee_name: None,
            returned_quantity: 0,
            serial_numbers: vec![],
        }],
        subtotal_cents: line_total_cents,
        discount_type: "fixed".to_string(),
        discount_value: 0.0,
        discount_cents: 0,
        total_cents: line_total_cents,
        payment_method: "cash".to_string(),
        is_credit: false,
        amount_received_cents: Some(line_total_cents),
        change_due_cents: Some(0),
        split_payments: None,
        card_last4: None,
        card_ref: None,
        online_ref: None,
        online_note: None,
        due_date: None,
        status: InvoiceStatus::Paid,
        refunded_cents: 0,
        credit_note_count: 0,
        voided_at: None,
        voided_by: None,
        voided_reason: None,
        closed_at: None,
        closed_by: None,
        shop_profile_snapshot: json!({}),
        notes: None,
        warranty_terms_snapshot: None,
        document_selection: None,
        version: 1,
        created_at: BsonDateTime::from_chrono(created_at),
        updated_at: BsonDateTime::from_chrono(created_at),
    }
}

#[tokio::test]
async fn test_engine_feed_overview_and_cache_hit() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    // Seed test invoice
    let now = Utc::now();
    let invoice = seed_invoice("inv_engine_1", now, 20_000, 10_000);
    app.db
        .collection::<InvoiceDocument>("invoices")
        .insert_one(invoice)
        .await
        .unwrap();

    // First request: Cold run (Cache MISS)
    let (status, body1) = send_authed(
        &app.router,
        "GET",
        "/api/reports/engine/feed?section=overview&preset=this_month",
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body1}");
    assert_eq!(body1["success"], true);
    assert_eq!(body1["data"]["section"], "overview");
    assert!(body1["data"]["overview"]["summary"].is_object());
    assert!(body1["data"]["overview"]["timeseries"].is_object());
    assert!(body1["data"]["overview"]["paymentMethods"].is_object());
    assert!(body1["data"]["overview"]["salesPatterns"].is_object());
    assert_eq!(body1["data"]["meta"]["cacheHit"], false);

    // Second request: Hot run (Cache HIT)
    let (status2, body2) = send_authed(
        &app.router,
        "GET",
        "/api/reports/engine/feed?section=overview&preset=this_month",
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status2, StatusCode::OK, "{body2}");
    assert_eq!(body2["data"]["meta"]["cacheHit"], true);

    // Invalidation endpoint
    let (status_inv, body_inv) = send_authed(
        &app.router,
        "POST",
        "/api/reports/engine/invalidate",
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status_inv, StatusCode::OK, "{body_inv}");
    assert_eq!(body_inv["success"], true);
}

#[tokio::test]
async fn test_engine_feed_sales_and_profit_sections() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    // Sales section
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/engine/feed?section=sales&preset=this_month",
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["section"], "sales");
    assert!(body["data"]["sales"]["dailySales"].is_object());
    assert!(body["data"]["sales"]["salesByCategory"].is_object());
    assert!(body["data"]["sales"]["discounts"].is_object());
    assert!(body["data"]["sales"]["refunds"].is_object());
    assert!(body["data"]["sales"]["topProducts"].is_object());

    // Profit section
    let (status_p, body_p) = send_authed(
        &app.router,
        "GET",
        "/api/reports/engine/feed?section=profit&preset=this_month",
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status_p, StatusCode::OK, "{body_p}");
    assert_eq!(body_p["data"]["section"], "profit");
    assert!(body_p["data"]["profit"]["monthlyProfit"].is_object());
    assert!(body_p["data"]["profit"]["timeseries"].is_object());
}

#[test]
fn test_chart_vectorizer_enrichment() {
    let now = Utc::now();
    let points = vec![
        TimeSeriesPoint {
            period_start: now,
            label: "Day 1".to_string(),
            revenue_cents: 10_000,
            retail_revenue_cents: 10_000,
            repair_revenue_cents: 0,
            print_revenue_cents: 0,
            discount_cents: 0,
            cogs_cents: 5_000,
            gross_profit_cents: 5_000,
            gross_margin_bps: 5000,
            commission_cents: 0,
            net_profit_cents: 5_000,
            invoice_count: 1,
        },
        TimeSeriesPoint {
            period_start: now + Duration::days(1),
            label: "Day 2".to_string(),
            revenue_cents: 20_000,
            retail_revenue_cents: 20_000,
            repair_revenue_cents: 0,
            print_revenue_cents: 0,
            discount_cents: 0,
            cogs_cents: 8_000,
            gross_profit_cents: 12_000,
            gross_margin_bps: 6000,
            commission_cents: 0,
            net_profit_cents: 12_000,
            invoice_count: 2,
        },
    ];

    let enriched = enrich_timeseries(&points, 2);
    assert_eq!(enriched.len(), 2);
    assert_eq!(enriched[0].cumulative_revenue_cents, 10_000);
    assert_eq!(enriched[1].cumulative_revenue_cents, 30_000);
    assert_eq!(enriched[1].cumulative_profit_cents, 17_000);
    assert_eq!(enriched[1].moving_avg_revenue_cents, Some(15_000));
}

#[tokio::test]
async fn test_pairwise_preset_configurations() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    // 24-vector Pairwise (All-Pairs) test matrix covering all 2-way factor interactions:
    // 7 Presets x 6 Sections x 3 Granularities (day, week, month)
    let pairwise_matrix = vec![
        ("today", "overview", "day"),
        ("today", "sales", "week"),
        ("today", "profit", "month"),
        ("yesterday", "customers", "day"),
        ("yesterday", "staff", "week"),
        ("yesterday", "all", "month"),
        ("this_week", "overview", "week"),
        ("this_week", "sales", "month"),
        ("this_week", "profit", "day"),
        ("this_month", "customers", "week"),
        ("this_month", "staff", "month"),
        ("this_month", "all", "day"),
        ("last_month", "overview", "month"),
        ("last_month", "sales", "day"),
        ("last_month", "profit", "week"),
        ("this_year", "customers", "month"),
        ("this_year", "staff", "day"),
        ("this_year", "all", "week"),
        ("all_time", "overview", "day"),
        ("all_time", "sales", "week"),
        ("all_time", "profit", "month"),
        ("all_time", "customers", "day"),
        ("all_time", "staff", "week"),
        ("all_time", "all", "month"),
    ];

    for (preset, section, granularity) in pairwise_matrix {
        let uri = format!(
            "/api/reports/engine/feed?section={section}&preset={preset}&granularity={granularity}"
        );
        let (status, body) = send_authed(&app.router, "GET", &uri, None, &admin_token).await;

        assert_eq!(
            status,
            StatusCode::OK,
            "Failed pairwise test for preset '{preset}', section '{section}', granularity '{granularity}': {body}"
        );
        assert_eq!(body["success"], true);
        assert_eq!(body["data"]["section"], section);
        assert!(body["data"]["meta"]["executionTimeMs"].as_u64().is_some());

        // Validate section data structure payload
        match section {
            "overview" => {
                assert!(body["data"]["overview"]["summary"].is_object());
                assert!(body["data"]["overview"]["timeseries"].is_object());
                assert!(body["data"]["overview"]["paymentMethods"].is_object());
                assert!(body["data"]["overview"]["salesPatterns"].is_object());
            }
            "sales" => {
                assert!(body["data"]["sales"]["dailySales"].is_object());
                assert!(body["data"]["sales"]["salesByCategory"].is_object());
                assert!(body["data"]["sales"]["discounts"].is_object());
                assert!(body["data"]["sales"]["refunds"].is_object());
                assert!(body["data"]["sales"]["topProducts"].is_object());
            }
            "profit" => {
                assert!(body["data"]["profit"]["monthlyProfit"].is_object());
                assert!(body["data"]["profit"]["timeseries"].is_object());
            }
            "customers" => {
                assert!(body["data"]["customers"]["topCustomers"].is_object());
                assert!(body["data"]["customers"]["receivablesAging"].is_object());
            }
            "staff" => {
                assert!(body["data"]["staff"]["employeeCommissions"].is_object());
                assert!(body["data"]["staff"]["cashierPerformance"].is_object());
            }
            "all" => {
                assert!(body["data"]["overview"].is_object());
                assert!(body["data"]["sales"].is_object());
                assert!(body["data"]["profit"].is_object());
                assert!(body["data"]["customers"].is_object());
                assert!(body["data"]["staff"].is_object());
            }
            _ => panic!("Unexpected section: {section}"),
        }
    }
}

#[tokio::test]
async fn test_custom_date_boundary_and_equivalence() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    let boundary_cases = vec![
        // 1. Single Calendar Day boundary
        (
            "single_day",
            "/api/reports/engine/feed?preset=custom&from=2026-08-15&to=2026-08-15&section=overview",
        ),
        // 2. Multi-Year Long Range boundary
        (
            "multi_year",
            "/api/reports/engine/feed?preset=custom&from=2020-01-01&to=2026-12-31&section=sales",
        ),
        // 3. Leap Year bounds (February 28 to 29)
        (
            "leap_year_day",
            "/api/reports/engine/feed?preset=custom&from=2024-02-28&to=2024-02-29&section=profit",
        ),
        // 4. Leap Year rollover (February 29 to March 1)
        (
            "leap_year_rollover",
            "/api/reports/engine/feed?preset=custom&from=2024-02-29&to=2024-03-01&section=customers",
        ),
        // 5. Century Leap Year (Year 2000)
        (
            "century_leap_year",
            "/api/reports/engine/feed?preset=custom&from=2000-02-28&to=2000-02-29&section=staff",
        ),
        // 6. Sub-Second RFC3339 timestamps (Microseconds window)
        (
            "sub_second_rfc3339",
            "/api/reports/engine/feed?preset=custom&from=2026-08-15T09:00:00.123456Z&to=2026-08-15T09:00:00.999999Z&section=overview",
        ),
        // 7. Timezone Offset with Milliseconds RFC3339
        (
            "tz_offset_rfc3339",
            "/api/reports/engine/feed?preset=custom&from=2026-08-15T14:30:00.000%2B05:30&to=2026-08-15T23:59:59.999%2B05:30&section=overview",
        ),
        // 8. Partial Equivalence: from only
        (
            "partial_from_only",
            "/api/reports/engine/feed?preset=custom&from=2026-08-01&section=overview",
        ),
        // 9. Partial Equivalence: to only
        (
            "partial_to_only",
            "/api/reports/engine/feed?preset=custom&to=2026-08-31&section=overview",
        ),
        // 10. Omitted from and to with preset=custom
        (
            "custom_empty_bounds",
            "/api/reports/engine/feed?preset=custom&section=all",
        ),
    ];

    for (name, uri) in boundary_cases {
        let (status, body) = send_authed(&app.router, "GET", uri, None, &admin_token).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "Boundary case '{name}' failed at URI '{uri}': {body}"
        );
        assert_eq!(
            body["success"], true,
            "Case '{name}' did not return success"
        );
        assert!(
            body["data"]["meta"].is_object(),
            "Case '{name}' missing meta object"
        );
    }
}

#[tokio::test]
async fn test_invalid_date_error_handling() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    let negative_cases = vec![
        // 1. Inverted calendar date range (start after end)
        (
            "/api/reports/engine/feed?preset=custom&from=2026-08-30&to=2026-08-01",
            "Start date cannot be after end date",
        ),
        // 2. Inverted RFC3339 timestamps
        (
            "/api/reports/engine/feed?preset=custom&from=2026-08-30T10:00:00Z&to=2026-08-01T10:00:00Z",
            "Start date cannot be after end date",
        ),
        // 3. Malformed calendar date string
        (
            "/api/reports/engine/feed?preset=custom&from=invalid-date&to=2026-08-30",
            "Invalid date format",
        ),
        // 4. Invalid month number in YYYY-MM-DD
        (
            "/api/reports/engine/feed?preset=custom&from=2026-13-45&to=2026-08-30",
            "Invalid date format",
        ),
        // 5. Invalid day in 31-day month
        (
            "/api/reports/engine/feed?preset=custom&from=2026-08-01&to=2026-08-32",
            "Invalid date format",
        ),
        // 6. Invalid leap day in non-leap year (2025-02-29)
        (
            "/api/reports/engine/feed?preset=custom&from=2025-02-29&to=2025-03-01",
            "Invalid date format",
        ),
        // 7. Invalid leap day in non-leap year (2023-02-29)
        (
            "/api/reports/engine/feed?preset=custom&from=2023-02-29&to=2023-03-01",
            "Invalid date format",
        ),
        // 8. Unknown date preset parameter
        (
            "/api/reports/engine/feed?preset=next_quarter",
            "Unknown date preset 'next_quarter'",
        ),
        // 9. Malformed RFC3339 timestamp (invalid hour 25)
        (
            "/api/reports/engine/feed?preset=custom&from=2026-08-15T25:00:00Z&to=2026-08-16T00:00:00Z",
            "Invalid date format",
        ),
        // 10. Malformed RFC3339 timestamp (invalid minute 60)
        (
            "/api/reports/engine/feed?preset=custom&from=2026-08-15T12:60:00Z&to=2026-08-16T00:00:00Z",
            "Invalid date format",
        ),
        // 11. Invalid granularity parameter
        (
            "/api/reports/engine/feed?preset=today&granularity=century",
            "Unknown granularity 'century'",
        ),
    ];

    for (uri, expected_err_fragment) in negative_cases {
        let (status, body) = send_authed(&app.router, "GET", uri, None, &admin_token).await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Expected 400 Bad Request for URI '{uri}', but got {status}: {body}"
        );
        assert_eq!(body["success"], false);
        assert_eq!(body["code"], "VALIDATION_ERROR");
        let msg = body["message"].as_str().unwrap_or("");
        assert!(
            msg.contains(expected_err_fragment),
            "Expected message to contain '{expected_err_fragment}', but got '{msg}' for URI '{uri}'"
        );
    }
}
