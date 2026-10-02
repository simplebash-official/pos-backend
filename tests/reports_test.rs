mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use chrono::{Duration, NaiveDate, Utc};
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    core::{constants::roles, id::generate_id},
    domain::{billing::InvoiceStatus, users::Role},
    modules::{
        billing::model::{InvoiceDocument, PaymentDocument},
        inventory::model::ProductDocument,
        print_jobs::model::PrintJobDocument,
        repairs::model::RepairDocument,
    },
};
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
            simplebash_pos_backend::domain::billing::InvoiceItem {
                product_key: Some("prd_phone_case".to_string()),
                name: "Phone Case".to_string(),
                sku: Some("PHO-CAS-0001".to_string()),
                unit_price_cents: 150000,
                quantity: 2,
                discount_cents: 0,
                total_cents: 300000,
                unit_cost_cents: Some(90000),
                source_type: "retail".to_string(),
                source_ticket_key: None,
                source_ticket_number: None,
                assigned_employee_name: None,
                returned_quantity: 0,
                serial_numbers: vec![],
            },
            simplebash_pos_backend::domain::billing::InvoiceItem {
                product_key: None,
                name: "Screen Repair Service".to_string(),
                sku: None,
                unit_price_cents: 500000,
                quantity: 1,
                discount_cents: 50000,
                total_cents: 450000,
                unit_cost_cents: Some(30000),
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
        items: vec![simplebash_pos_backend::domain::billing::InvoiceItem {
            product_key: None,
            name: "T-Shirt Sublimation Print".to_string(),
            sku: None,
            unit_price_cents: 200000,
            quantity: 1,
            discount_cents: 0,
            total_cents: 200000,
            unit_cost_cents: Some(40000),
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
        items: vec![simplebash_pos_backend::domain::billing::InvoiceItem {
            product_key: Some("prd_phone_screen".to_string()),
            name: "Replacement Screen Part".to_string(),
            sku: Some("PHO-SCR-0002".to_string()),
            unit_price_cents: 1000000,
            quantity: 1,
            discount_cents: 0,
            total_cents: 1000000,
            unit_cost_cents: Some(650000),
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
        promised_ready_at: None,
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
        promised_ready_at: None,
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

/// Regression test for the free-text-name grouping bug: commissions used to
/// be bucketed by `assigned_employee_name`, so a renamed employee's history
/// split across two entries instead of staying merged under their real
/// identity. Now bucketed by `assigned_employee_id` (a real `employees`
/// key), so two tickets completed under the same employee — one before and
/// one after a rename — must still merge into a single commissions entry.
#[tokio::test]
async fn employee_commissions_survive_a_rename() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/employees",
        Some(json!({
            "name": "Rename Regression Tech",
            "phone": "0771234567",
            "role": "technician",
            "defaultSplitType": "percentage",
            "defaultSplitValue": 30.0,
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let employee_id = body["data"]["id"].as_str().unwrap().to_string();
    let employee_key = body["data"]["key"].as_str().unwrap().to_string();

    let target_date = NaiveDate::from_ymd_opt(2026, 8, 20)
        .unwrap()
        .and_hms_opt(11, 0, 0)
        .unwrap()
        .and_utc();

    let make_repair = |key_suffix: &str, name: &str| RepairDocument {
        id: None,
        key: generate_id("rep"),
        ticket_number: format!("REP-{key_suffix}"),
        customer_key: None,
        customer_name: name.to_string(),
        customer_phone: "0770000000".to_string(),
        device_model: "iPhone 13".to_string(),
        serial_number: None,
        issue_description: "Screen cracked".to_string(),
        promised_ready_at: None,
        status: "delivered".to_string(),
        estimated_cost_cents: Some(500000),
        material_cost_cents: Some(200000),
        assigned_employee_id: Some(employee_key.clone()),
        assigned_employee_name: Some("stale snapshot, must be ignored".to_string()),
        split_type: Some("percentage".to_string()),
        split_value: Some(30.0),
        version: 1,
        created_at: BsonDateTime::from_chrono(target_date),
        updated_at: BsonDateTime::from_chrono(target_date),
        deleted_at: None,
        updated_by_device: None,
    };

    app.db
        .collection::<RepairDocument>("repairs")
        .insert_one(make_repair("R1", "Customer Before Rename"))
        .await
        .unwrap();

    // Rename the employee, then complete a second ticket under the same id.
    let (status, body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/employees/{employee_id}"),
        Some(json!({ "name": "Renamed Regression Tech" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    app.db
        .collection::<RepairDocument>("repairs")
        .insert_one(make_repair("R2", "Customer After Rename"))
        .await
        .unwrap();

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!(
            "/api/reports/employee-commissions?from=2026-08-20&to=2026-08-20&employeeKey={employee_key}"
        ),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let entries = body["data"]["employees"].as_array().unwrap();
    assert_eq!(
        entries.len(),
        1,
        "both tickets must merge into one entry keyed by employee id, not split by name: {entries:?}"
    );
    assert_eq!(entries[0]["employeeName"], "Renamed Regression Tech");
    assert_eq!(entries[0]["assignedJobsCount"], 2);
}

// ---------------------------------------------------------------------------
// Analytics & Reports — /reports/analytics/*
// ---------------------------------------------------------------------------

/// Minimal paid retail invoice for analytics seeding. `unit_cost_cents` is set
/// on the single line so retail COGS / gross profit / coverage can be asserted.
#[allow(clippy::too_many_arguments)]
fn analytics_seed_invoice(
    key: &str,
    created_at: chrono::DateTime<Utc>,
    line_total_cents: i64,
    unit_cost_cents: i64,
    quantity: i64,
    invoice_discount_cents: i64,
) -> InvoiceDocument {
    use simplebash_pos_backend::domain::billing::InvoiceItem;
    let net = (line_total_cents - invoice_discount_cents).max(0);
    InvoiceDocument {
        id: None,
        key: key.to_string(),
        invoice_number: format!("INV-{}", generate_id("t")),
        customer_key: None,
        customer_name_snapshot: Some("Walk-in".to_string()),
        customer_phone_snapshot: None,
        customer_address_snapshot: None,
        cashier_id: "usr_cashier_ax".to_string(),
        cashier_name_snapshot: "Cashier AX".to_string(),
        items: vec![InvoiceItem {
            product_key: Some(format!("prd_{key}")),
            name: "Analytics Test Item".to_string(),
            sku: Some("AX-TST-0001".to_string()),
            unit_price_cents: line_total_cents / quantity.max(1),
            quantity,
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
        discount_value: (invoice_discount_cents as f64) / 100.0,
        discount_cents: invoice_discount_cents,
        total_cents: net,
        payment_method: "cash".to_string(),
        is_credit: false,
        amount_received_cents: Some(net),
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
async fn analytics_summary_and_timeseries_use_cost_snapshot_and_shop_local_buckets() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let staff_token = common::mint_token(
        &app.config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );

    // A quiet future window no other test seeds into. Two shop-local days:
    // 15 Jun (15:30 Colombo) and 16 Jun (09:30 Colombo).
    let day1 = NaiveDate::from_ymd_opt(2027, 6, 15)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap()
        .and_utc();
    let day2 = NaiveDate::from_ymd_opt(2027, 6, 16)
        .unwrap()
        .and_hms_opt(4, 0, 0)
        .unwrap()
        .and_utc();

    // Clear anything (from a prior run) that the analytics aggregations read
    // across in this window: invoices, and the repair/print jobs + credit
    // notes that feed service material cost and refunds.
    let window = doc! {
        "created_at": {
            "$gte": BsonDateTime::from_chrono(day1 - Duration::days(2)),
            "$lte": BsonDateTime::from_chrono(day2 + Duration::days(2)),
        }
    };
    for coll in ["invoices", "repairs", "print_jobs", "credit_notes"] {
        let _ = app
            .db
            .collection::<Document>(coll)
            .delete_many(window.clone())
            .await;
    }

    // A real product + category so sales-by-category can resolve a name via
    // the `categories` `$lookup` (the `products` collection stores only the
    // key). Upserted rather than window-scoped since these have no useful
    // `created_at` filter.
    let cat_key = "cat_ax_widgets";
    let prd_key = "prd_ax_widget";
    let _ = app
        .db
        .collection::<Document>("categories")
        .update_one(
            doc! { "key": cat_key },
            doc! { "$set": { "key": cat_key, "name": "AX Widgets", "icon": "", "color": "" } },
        )
        .upsert(true)
        .await;
    let _ = app
        .db
        .collection::<Document>("products")
        .update_one(
            doc! { "key": prd_key },
            doc! { "$set": {
                "key": prd_key,
                "category_key": cat_key,
                "subcategory_key": "sub_ax",
                "cost_price_cents": 40_000_i64,
                "selling_price_cents": 100_000_i64,
            } },
        )
        .upsert(true)
        .await;

    let invoices_coll = app.db.collection::<InvoiceDocument>("invoices");

    let mut categorized =
        analytics_seed_invoice(&generate_id("axinv"), day1, 200_000, 40_000, 1, 0);
    categorized.items[0].product_key = Some(prd_key.to_string());

    invoices_coll
        .insert_many(vec![
            analytics_seed_invoice(&generate_id("axinv"), day1, 300_000, 90_000, 2, 0),
            analytics_seed_invoice(&generate_id("axinv"), day2, 100_000, 60_000, 1, 10_000),
            categorized,
        ])
        .await
        .unwrap();

    // Staff (no reports:view) is rejected.
    let (status, _) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/summary?preset=custom&from=2027-06-15&to=2027-06-16",
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Summary: retail COGS + gross profit come from the per-line cost snapshot.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/summary?preset=custom&from=2027-06-15&to=2027-06-16&comparePrevious=true",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let cur = &body["data"]["current"];
    assert_eq!(cur["totalRevenueCents"], 590_000); // 300000 + (100000 - 10000) + 200000
    assert_eq!(cur["retailCogsCents"], 280_000); // 90000*2 + 60000*1 + 40000*1
    assert_eq!(cur["grossProfitCents"], 310_000); // 590000 - 280000
    assert_eq!(cur["itemsSold"], 4);
    assert_eq!(cur["invoiceCount"], 3);
    assert_eq!(cur["cogsCoverageBps"], 10_000); // every retail line had a cost
    assert!(body["data"]["previous"].is_object());
    assert!(body["data"]["deltas"].is_object());

    // Time series: two daily buckets, one per shop-local day, gap-free.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/timeseries?preset=custom&from=2027-06-15&to=2027-06-16&granularity=day",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["granularity"], "day");
    let points = body["data"]["points"].as_array().unwrap();
    assert_eq!(points.len(), 2, "one bucket per shop-local day: {points:?}");
    assert_eq!(points[0]["revenueCents"], 500_000); // day1: 300000 + 200000
    assert_eq!(points[1]["revenueCents"], 100_000); // day2 line total, pre-invoice-discount
    assert_eq!(points[1]["cogsCents"], 60_000);
    assert_eq!(body["data"]["totals"]["revenueCents"], 600_000);

    // Payment methods: every invoice was cash.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/payment-methods?preset=custom&from=2027-06-15&to=2027-06-16",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["breakdown"]["cashCents"], 590_000);
    assert_eq!(body["data"]["totalCents"], 590_000);

    // Top customers: all walk-in → one collapsed row.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/top-customers?preset=custom&from=2027-06-15&to=2027-06-16",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let customers = body["data"]["customers"].as_array().unwrap();
    assert_eq!(customers.len(), 1);
    assert_eq!(customers[0]["isWalkIn"], true);
    assert_eq!(customers[0]["invoiceCount"], 3);
    // retail line revenue (300000 + 100000 + 200000) − cogs (180000 + 60000 + 40000)
    assert_eq!(customers[0]["grossProfitCents"], 320_000);

    // Cashier performance: one cashier, both invoices.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/cashier-performance?preset=custom&from=2027-06-15&to=2027-06-16",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let cashiers = body["data"]["cashiers"].as_array().unwrap();
    assert_eq!(cashiers.len(), 1);
    assert_eq!(cashiers[0]["cashierId"], "usr_cashier_ax");
    assert_eq!(cashiers[0]["invoiceCount"], 3);
    assert_eq!(cashiers[0]["itemsSold"], 4);

    // Sales by category: the categorised line resolves its name via the
    // `categories` lookup; the two `prd_axinv…` lines don't resolve a product
    // and land in `uncategorised`.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/sales-by-category?preset=custom&from=2027-06-15&to=2027-06-16",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["uncategorised"]["revenueCents"], 400_000); // 300000 + 100000
    assert_eq!(body["data"]["totalRevenueCents"], 600_000);
    let rows = body["data"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["categoryName"], "AX Widgets");
    assert_eq!(rows[0]["revenueCents"], 200_000);

    // Sales patterns: 168 zero-filled cells, both sales on the same weekday cell counted.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/sales-patterns?preset=custom&from=2027-06-15&to=2027-06-16",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["cells"].as_array().unwrap().len(), 168);
    let total_pattern_invoices: u64 = body["data"]["byWeekday"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["invoiceCount"].as_u64().unwrap())
        .sum();
    assert_eq!(total_pattern_invoices, 3);

    // Discounts: one invoice had a 10000 order-level discount.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/discounts?preset=custom&from=2027-06-15&to=2027-06-16",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["orderLevelDiscountCents"], 10_000);
    assert_eq!(body["data"]["invoicesWithDiscount"], 1);
}

/// The analytics PDF endpoint round-trips backend route -> payload builder ->
/// document-server client. `spawn_app`'s mock document-server publishes an
/// "Analytics Report" template and returns stub PDF bytes, so this exercises
/// the whole backend path (aggregation, payload shaping, schema pre-validation,
/// headers) without needing the real Typst renderer.
#[tokio::test]
async fn analytics_document_returns_a_pdf() {
    let app = common::spawn_app().await;
    let admin_token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    let request = Request::builder()
        .method("GET")
        .uri("/api/reports/analytics/document?preset=this_month&granularity=week")
        .header(AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/pdf")
    );
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(bytes.starts_with(b"%PDF"), "response body should be a PDF");
}

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
    body["data"]["key"].as_str().unwrap().to_string()
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
    body["data"]["invoice"]["key"].as_str().unwrap().to_string()
}

fn find_reminder<'a>(body: &'a Value, key: &str) -> Option<&'a Value> {
    body["data"]["reminders"]
        .as_array()
        .unwrap()
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
            "estimatedCostCents": 400_000,
            "promisedReadyAt": date_offset(-2),
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {repair_body}");
    let repair_key = repair_body["data"]["key"].as_str().unwrap().to_string();

    let (status, print_body) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(json!({
            "customer": { "customerName": "Print Reminder" },
            "jobType": "banner",
            "quantity": 2,
            "estimatedCostCents": 90_000,
            "promisedReadyAt": date_offset(1),
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {print_body}");
    let print_key = print_body["data"]["key"].as_str().unwrap().to_string();

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
            "estimatedCostCents": 400_000,
            "promisedReadyAt": date_offset(-10),
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {repair_body}");
    let repair_key = repair_body["data"]["key"].as_str().unwrap().to_string();

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

    let reminders = body["data"]["reminders"].as_array().unwrap();
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
    let total = body1["data"]["total"].as_u64().unwrap();
    assert!(total >= 3);
    assert!(body1["data"]["totalPages"].as_u64().unwrap() >= 2);
    let page1_reminders = body1["data"]["reminders"].as_array().unwrap();
    assert_eq!(page1_reminders.len(), 2);
    let page1_keys: Vec<String> = page1_reminders
        .iter()
        .map(|r| r["key"].as_str().unwrap().to_string())
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
    let page2_reminders = body2["data"]["reminders"].as_array().unwrap();
    assert!(!page2_reminders.is_empty());
    let page2_keys: Vec<String> = page2_reminders
        .iter()
        .map(|r| r["key"].as_str().unwrap().to_string())
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
