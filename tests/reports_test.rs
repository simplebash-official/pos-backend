mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use chrono::{Duration, NaiveDate, Utc};
use jana2u_pos_backend::{
    core::{constants::roles, id::generate_id},
    domain::{billing::InvoiceStatus, users::Role},
    modules::{
        billing::model::{InvoiceDocument, PaymentDocument},
        inventory::model::ProductDocument,
        print_jobs::model::PrintJobDocument,
        repairs::model::RepairDocument,
    },
};
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
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

async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    execute(router, request).await
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

#[tokio::test]
async fn reports_module_status_is_public() {
    let app = common::spawn_app().await;
    let (status, body) = send(&app.router, "GET", "/api/reports", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["module"], "reports");
    assert_eq!(body["data"]["status"], "ok");
}

#[tokio::test]
async fn unauthenticated_requests_to_reports_endpoints_return_401() {
    let app = common::spawn_app().await;
    let endpoints = [
        "/api/reports/dashboard",
        "/api/reports/daily-sales",
        "/api/reports/monthly-profit",
        "/api/reports/outstanding",
        "/api/reports/employee-commissions",
        "/api/reports/inventory-valuation",
        "/api/reports/top-products",
    ];

    for endpoint in endpoints {
        let (status, _) = send(&app.router, "GET", endpoint, None).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "Endpoint {endpoint} should require auth"
        );
    }
}

#[tokio::test]
async fn staff_role_without_reports_view_permission_returns_403() {
    let app = common::spawn_app().await;
    let staff_token = common::mint_token(
        &app.config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/dashboard",
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "PERMISSION_DENIED");
}

#[tokio::test]
async fn admin_and_manager_tokens_with_reports_view_permission_return_200() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let manager_token = common::mint_token(
        &app.config,
        Some(Role::Manager),
        roles::default_permissions(Role::Manager),
    );

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/dashboard",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/dashboard",
        None,
        &manager_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn dashboard_overview_and_daily_sales_aggregate_data() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    // Use a dedicated date bound for test isolation
    let target_date = NaiveDate::from_ymd_opt(2026, 8, 15)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap()
        .and_utc();

    // Clean up prior test runs for this specific target date window
    let _ = app
        .db
        .collection::<Document>("invoices")
        .delete_many(doc! {
            "created_at": {
                "$gte": BsonDateTime::from_chrono(target_date - Duration::days(1)),
                "$lte": BsonDateTime::from_chrono(target_date + Duration::days(1)),
            }
        })
        .await;
    let _ = app
        .db
        .collection::<Document>("payments")
        .delete_many(doc! {
            "created_at": {
                "$gte": BsonDateTime::from_chrono(target_date - Duration::days(1)),
                "$lte": BsonDateTime::from_chrono(target_date + Duration::days(1)),
            }
        })
        .await;

    let invoices_coll = app.db.collection::<InvoiceDocument>("invoices");
    let payments_coll = app.db.collection::<PaymentDocument>("payments");

    let inv1_key = generate_id("inv");
    let inv1 = InvoiceDocument {
        id: None,
        key: inv1_key.clone(),
        invoice_number: format!("INV-{}", generate_id("t")),
        customer_key: None,
        customer_name_snapshot: Some("John Doe".to_string()),
        customer_phone_snapshot: Some("0771234567".to_string()),
        customer_address_snapshot: None,
        cashier_id: "usr_cashier1".to_string(),
        cashier_name_snapshot: "Cashier One".to_string(),
        items: vec![
            jana2u_pos_backend::domain::billing::InvoiceItem {
                product_key: Some("prd_phone_case".to_string()),
                name: "Phone Case".to_string(),
                sku: Some("PHO-CAS-0001".to_string()),
                unit_price_cents: 150000,
                quantity: 2,
                discount_cents: 0,
                total_cents: 300000,
                source_type: "retail".to_string(),
                source_ticket_key: None,
                source_ticket_number: None,
                assigned_employee_name: None,
                returned_quantity: 0,
                serial_numbers: vec![],
            },
            jana2u_pos_backend::domain::billing::InvoiceItem {
                product_key: None,
                name: "Screen Repair Service".to_string(),
                sku: None,
                unit_price_cents: 500000,
                quantity: 1,
                discount_cents: 50000,
                total_cents: 450000,
                source_type: "repair".to_string(),
                source_ticket_key: Some("rep_101".to_string()),
                source_ticket_number: Some("REP-000101".to_string()),
                assigned_employee_name: Some("Tech Nimal".to_string()),
                returned_quantity: 0,
                serial_numbers: vec![],
            },
        ],
        subtotal_cents: 800000,
        discount_type: "fixed".to_string(),
        discount_value: 500.0,
        discount_cents: 50000,
        total_cents: 750000,
        payment_method: "cash".to_string(),
        is_credit: false,
        amount_received_cents: Some(800000),
        change_due_cents: Some(50000),
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
        created_at: BsonDateTime::from_chrono(target_date),
        updated_at: BsonDateTime::from_chrono(target_date),
    };

    let inv2_key = generate_id("inv");
    let inv2 = InvoiceDocument {
        id: None,
        key: inv2_key.clone(),
        invoice_number: format!("INV-{}", generate_id("t")),
        customer_key: None,
        customer_name_snapshot: Some("Jane Smith".to_string()),
        customer_phone_snapshot: Some("0719876543".to_string()),
        customer_address_snapshot: None,
        cashier_id: "usr_cashier1".to_string(),
        cashier_name_snapshot: "Cashier One".to_string(),
        items: vec![jana2u_pos_backend::domain::billing::InvoiceItem {
            product_key: None,
            name: "T-Shirt Sublimation Print".to_string(),
            sku: None,
            unit_price_cents: 200000,
            quantity: 1,
            discount_cents: 0,
            total_cents: 200000,
            source_type: "print".to_string(),
            source_ticket_key: Some("prj_202".to_string()),
            source_ticket_number: Some("PRN-000202".to_string()),
            assigned_employee_name: Some("Printer Suneth".to_string()),
            returned_quantity: 0,
            serial_numbers: vec![],
        }],
        subtotal_cents: 200000,
        discount_type: "fixed".to_string(),
        discount_value: 0.0,
        discount_cents: 0,
        total_cents: 200000,
        payment_method: "card".to_string(),
        is_credit: false,
        amount_received_cents: Some(200000),
        change_due_cents: Some(0),
        split_payments: None,
        card_last4: Some("4242".to_string()),
        card_ref: Some("TX1234".to_string()),
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
        created_at: BsonDateTime::from_chrono(target_date),
        updated_at: BsonDateTime::from_chrono(target_date),
    };

    // Credit invoice
    let inv3_key = generate_id("inv");
    let inv3 = InvoiceDocument {
        id: None,
        key: inv3_key.clone(),
        invoice_number: format!("INV-{}", generate_id("t")),
        customer_key: Some("cus_credit_cust".to_string()),
        customer_name_snapshot: Some("Credit Customer".to_string()),
        customer_phone_snapshot: Some("0755551234".to_string()),
        customer_address_snapshot: None,
        cashier_id: "usr_cashier1".to_string(),
        cashier_name_snapshot: "Cashier One".to_string(),
        items: vec![jana2u_pos_backend::domain::billing::InvoiceItem {
            product_key: Some("prd_phone_screen".to_string()),
            name: "Replacement Screen Part".to_string(),
            sku: Some("PHO-SCR-0002".to_string()),
            unit_price_cents: 1000000,
            quantity: 1,
            discount_cents: 0,
            total_cents: 1000000,
            source_type: "retail".to_string(),
            source_ticket_key: None,
            source_ticket_number: None,
            assigned_employee_name: None,
            returned_quantity: 0,
            serial_numbers: vec![],
        }],
        subtotal_cents: 1000000,
        discount_type: "fixed".to_string(),
        discount_value: 0.0,
        discount_cents: 0,
        total_cents: 1000000,
        payment_method: "credit".to_string(),
        is_credit: true,
        amount_received_cents: Some(400000), // partial payment
        change_due_cents: Some(0),
        split_payments: None,
        card_last4: None,
        card_ref: None,
        online_ref: None,
        online_note: None,
        due_date: Some("2026-08-10".to_string()), // Overdue relative to 2026-08-15
        status: InvoiceStatus::PartiallyPaid,
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
        created_at: BsonDateTime::from_chrono(target_date),
        updated_at: BsonDateTime::from_chrono(target_date),
    };

    invoices_coll
        .insert_many(vec![inv1, inv2, inv3])
        .await
        .unwrap();

    // Insert payment of 400000 against credit invoice
    let p1 = PaymentDocument {
        id: None,
        key: generate_id("pay"),
        invoice_key: inv3_key,
        amount_cents: 400000,
        payment_method: "cash".to_string(),
        notes: Some("Initial payment".to_string()),
        recorded_by_user_id: "usr_cashier1".to_string(),
        recorded_by_name_snapshot: "Cashier One".to_string(),
        recorded_at: BsonDateTime::from_chrono(target_date),
        version: 1,
        created_at: BsonDateTime::from_chrono(target_date),
        updated_at: BsonDateTime::from_chrono(target_date),
    };
    payments_coll.insert_one(p1).await.unwrap();

    // 1. Test Dashboard overview with custom range
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/dashboard?from=2026-08-15&to=2026-08-15",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["totalRevenueCents"], 1950000); // 750000 + 200000 + 1000000
    assert_eq!(data["retailRevenueCents"], 1300000); // 300000 + 1000000
    assert_eq!(data["repairRevenueCents"], 450000);
    assert_eq!(data["printRevenueCents"], 200000);
    assert_eq!(data["totalInvoicesCount"], 3);
    assert_eq!(data["totalDiscountsCents"], 50000);

    // 2. Test Daily Sales with custom date
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/daily-sales?date=2026-08-15",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["totalSalesCents"], 1950000);
    assert_eq!(data["totalInvoices"], 3);

    // 3. Test Outstanding Receivables
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/outstanding",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert!(data["totalOutstandingCents"].as_i64().unwrap() >= 600000);
    assert!(data["totalCreditInvoicesCount"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn employee_commissions_and_performance_report() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    let target_date = NaiveDate::from_ymd_opt(2026, 8, 16)
        .unwrap()
        .and_hms_opt(11, 0, 0)
        .unwrap()
        .and_utc();

    // Clean up prior test runs for this specific target date window
    let _ = app
        .db
        .collection::<Document>("repairs")
        .delete_many(doc! {
            "created_at": {
                "$gte": BsonDateTime::from_chrono(target_date - Duration::days(1)),
                "$lte": BsonDateTime::from_chrono(target_date + Duration::days(1)),
            }
        })
        .await;
    let _ = app
        .db
        .collection::<Document>("print_jobs")
        .delete_many(doc! {
            "created_at": {
                "$gte": BsonDateTime::from_chrono(target_date - Duration::days(1)),
                "$lte": BsonDateTime::from_chrono(target_date + Duration::days(1)),
            }
        })
        .await;

    // Completed repair with percentage split (30% on profit)
    let rep = RepairDocument {
        id: None,
        key: generate_id("rep"),
        ticket_number: format!("REP-{}", generate_id("t")),
        customer_key: None,
        customer_name: "Customer A".to_string(),
        customer_phone: "0770000000".to_string(),
        device_model: "iPhone 13".to_string(),
        serial_number: None,
        issue_description: "Screen cracked".to_string(),
        status: "delivered".to_string(),
        estimated_cost_cents: Some(500000), // Revenue = 5,000 LKR
        material_cost_cents: Some(200000),  // Cost = 2,000 LKR -> Profit = 3,000 LKR
        assigned_employee_id: Some("emp_1".to_string()),
        assigned_employee_name: Some("Unique Tech Nimal".to_string()),
        split_type: Some("percentage".to_string()),
        split_value: Some(30.0), // 30% of 3,000 = 900 LKR (90000 cents)
        version: 1,
        created_at: BsonDateTime::from_chrono(target_date),
        updated_at: BsonDateTime::from_chrono(target_date),
        deleted_at: None,
        updated_by_device: None,
    };

    app.db
        .collection::<RepairDocument>("repairs")
        .insert_one(rep)
        .await
        .unwrap();

    // Completed print job with fixed split (LKR 500 = 50000 cents)
    let prn = PrintJobDocument {
        id: None,
        key: generate_id("prj"),
        ticket_number: format!("PRN-{}", generate_id("t")),
        customer_key: None,
        customer_name: "Customer B".to_string(),
        customer_phone: None,
        job_type: "mug".to_string(),
        quantity: 10,
        status: "completed".to_string(),
        estimated_cost_cents: 300000,      // Revenue = 3,000 LKR
        material_cost_cents: Some(100000), // Cost = 1,000 LKR -> Profit = 2,000 LKR
        assigned_employee_id: Some("emp_2".to_string()),
        assigned_employee_name: Some("Unique Printer Suneth".to_string()),
        split_type: Some("fixed".to_string()),
        split_value: Some(50000.0), // 500 LKR commission
        version: 1,
        created_at: BsonDateTime::from_chrono(target_date),
        updated_at: BsonDateTime::from_chrono(target_date),
        deleted_at: None,
        updated_by_device: None,
    };

    app.db
        .collection::<PrintJobDocument>("print_jobs")
        .insert_one(prn)
        .await
        .unwrap();

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/employee-commissions?from=2026-08-16&to=2026-08-16",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["totalRevenueCents"], 800000);
    assert_eq!(data["totalCommissionsCents"], 140000); // 90000 + 50000
    assert_eq!(data["totalJobsCount"], 2);

    let employees = data["employees"].as_array().unwrap();
    assert_eq!(employees.len(), 2);
}

#[tokio::test]
async fn inventory_valuation_and_top_products_report() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    let now = Utc::now();
    let prod1 = ProductDocument {
        id: None,
        key: generate_id("prod"),
        sku: format!("VAL-{}", generate_id("s")),
        barcode: None,
        barcode_source: None,
        name: "Silicone Phone Case".to_string(),
        category_key: "cat_accessories".to_string(),
        subcategory_key: "subcat_cases".to_string(),
        cost_price_cents: 50000,
        selling_price_cents: 150000,
        stock_quantity: 20,
        min_stock_threshold: 5,
        is_serialized: false,
        warranty_months: None,
        version: 1,
        created_at: BsonDateTime::from_chrono(now),
        updated_at: BsonDateTime::from_chrono(now),
        deleted_at: None,
        updated_by_device: None,
    };

    let prod2 = ProductDocument {
        id: None,
        key: generate_id("prod"),
        sku: format!("VAL-{}", generate_id("s")),
        barcode: None,
        barcode_source: None,
        name: "OLED Replacement Screen".to_string(),
        category_key: "cat_parts".to_string(),
        subcategory_key: "subcat_screens".to_string(),
        cost_price_cents: 400000,
        selling_price_cents: 800000,
        stock_quantity: 3, // Low stock (<= 5)
        min_stock_threshold: 5,
        is_serialized: true,
        warranty_months: Some(6),
        version: 1,
        created_at: BsonDateTime::from_chrono(now),
        updated_at: BsonDateTime::from_chrono(now),
        deleted_at: None,
        updated_by_device: None,
    };

    app.db
        .collection::<ProductDocument>("products")
        .insert_many(vec![prod1, prod2])
        .await
        .unwrap();

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/inventory-valuation",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert!(data["totalProductsCount"].as_u64().unwrap() >= 2);
    assert!(data["totalStockUnits"].as_i64().unwrap() >= 23);
    assert!(data["totalCostValuationCents"].as_i64().unwrap() >= 2200000);
    assert!(data["totalRetailValuationCents"].as_i64().unwrap() >= 5400000);

    // Test top products endpoint
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/top-products?limit=5",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
}
