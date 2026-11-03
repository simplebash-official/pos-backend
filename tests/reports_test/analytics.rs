//! Analytics tests.

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use chrono::{Duration, NaiveDate, Utc};
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use serde_json::json;
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::id::generate_id;
use simplebash_pos_backend::domain::billing::InvoiceStatus;
use simplebash_pos_backend::domain::inventory::BatchSource;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::domain::warranties::ProductWarranty;
use simplebash_pos_backend::modules::billing::model::InvoiceDocument;
use simplebash_pos_backend::modules::inventory::model::{ProductDocument, StockBatchDocument};

use crate::common;
use crate::helpers::send_authed;
use simplebash_pos_backend::modules::repairs::model::RepairDocument;
use tower::ServiceExt;

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
            batch_allocations: vec![],
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
        session_key: None,
        version: 1,
        created_at: BsonDateTime::from_chrono(created_at),
        updated_at: BsonDateTime::from_chrono(created_at),
        updated_by_device: None,
    }
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
        stock_quantity: 20,
        min_stock_threshold: 5,
        is_serialized: false,
        warranty: ProductWarranty::none(),
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
        stock_quantity: 3, // Low stock (<= 5)
        min_stock_threshold: 5,
        is_serialized: true,
        warranty: simplebash_pos_backend::modules::warranties::migration::legacy_profile(Some(6)),
        version: 1,
        created_at: BsonDateTime::from_chrono(now),
        updated_at: BsonDateTime::from_chrono(now),
        deleted_at: None,
        updated_by_device: None,
    };

    // Stock is valued per batch: each product's units sit in one batch here.
    let batch = |product: &ProductDocument, cost: i64, price: i64| StockBatchDocument {
        id: None,
        key: generate_id("stb"),
        product_key: product.key.clone(),
        source: BatchSource::Opening,
        purchase_key: None,
        supplier_key: None,
        received_quantity: product.stock_quantity,
        remaining_quantity: product.stock_quantity,
        unit_cost_cents: cost,
        selling_price_cents: price,
        received_at: BsonDateTime::from_chrono(now),
        version: 1,
        created_at: BsonDateTime::from_chrono(now),
        updated_at: BsonDateTime::from_chrono(now),
        deleted_at: None,
        updated_by_device: None,
    };
    let batches = vec![batch(&prod1, 50000, 150000), batch(&prod2, 400000, 800000)];

    app.mongo()
        .collection::<ProductDocument>("products")
        .insert_many(vec![prod1, prod2])
        .await
        .expect("the awaited step succeeds");
    app.mongo()
        .collection::<StockBatchDocument>("stock_batches")
        .insert_many(batches)
        .await
        .expect("the awaited step succeeds");

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
    assert!(
        data["totalProductsCount"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 2
    );
    assert!(
        data["totalStockUnits"]
            .as_i64()
            .expect("the value is an integer")
            >= 23
    );
    assert!(
        data["totalCostValuationCents"]
            .as_i64()
            .expect("the value is an integer")
            >= 2200000
    );
    assert!(
        data["totalRetailValuationCents"]
            .as_i64()
            .expect("the value is an integer")
            >= 5400000
    );

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
    let employee_id = body["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let employee_key = body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let target_date = NaiveDate::from_ymd_opt(2026, 8, 20)
        .expect("the date is valid")
        .and_hms_opt(11, 0, 0)
        .expect("the date is valid")
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

    app.mongo()
        .collection::<RepairDocument>("repairs")
        .insert_one(make_repair("R1", "Customer Before Rename"))
        .await
        .expect("the awaited step succeeds");

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

    app.mongo()
        .collection::<RepairDocument>("repairs")
        .insert_one(make_repair("R2", "Customer After Rename"))
        .await
        .expect("the awaited step succeeds");

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

    let entries = body["data"]["employees"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(
        entries.len(),
        1,
        "both tickets must merge into one entry keyed by employee id, not split by name: {entries:?}"
    );
    assert_eq!(entries[0]["employeeName"], "Renamed Regression Tech");
    assert_eq!(entries[0]["assignedJobsCount"], 2);
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

    // A quiet past window no other test seeds into (the timeseries never
    // returns buckets after today, so it must not be in the future). Two shop-local days:
    // 15 Jun (15:30 Colombo) and 16 Jun (09:30 Colombo).
    let day1 = NaiveDate::from_ymd_opt(2021, 6, 15)
        .expect("the date is valid")
        .and_hms_opt(10, 0, 0)
        .expect("the date is valid")
        .and_utc();
    let day2 = NaiveDate::from_ymd_opt(2021, 6, 16)
        .expect("the date is valid")
        .and_hms_opt(4, 0, 0)
        .expect("the date is valid")
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
            .mongo()
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
    let now = mongodb::bson::DateTime::now();
    let _ = app
        .mongo()
        .collection::<Document>("categories")
        .update_one(
            doc! { "key": cat_key },
            doc! { "$set": {
                "key": cat_key, "name": "AX Widgets", "icon": "", "color": "",
                "version": 1_i64, "created_at": now, "updated_at": now,
            } },
        )
        .upsert(true)
        .await;
    let _ = app
        .mongo()
        .collection::<Document>("products")
        .update_one(
            doc! { "key": prd_key },
            doc! { "$set": {
                "key": prd_key,
                "category_key": cat_key,
                "subcategory_key": "sub_ax",
                "sku": "AX-WIDGET-001",
                "name": "AX Widget",
                "stock_quantity": 0_i64,
                "min_stock_threshold": 0_i64,
                "is_serialized": false,
                "version": 1_i64,
                "created_at": now,
                "updated_at": now,
            } },
        )
        .upsert(true)
        .await;

    let invoices_coll = app.mongo().collection::<InvoiceDocument>("invoices");

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
        .expect("the awaited step succeeds");

    // Staff (no reports:view) is rejected.
    let (status, _) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/summary?preset=custom&from=2021-06-15&to=2021-06-16",
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Summary: retail COGS + gross profit come from the per-line cost snapshot.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/summary?preset=custom&from=2021-06-15&to=2021-06-16&comparePrevious=true",
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
        "/api/reports/analytics/timeseries?preset=custom&from=2021-06-15&to=2021-06-16&granularity=day",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["granularity"], "day");
    let points = body["data"]["points"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(points.len(), 2, "one bucket per shop-local day: {points:?}");
    assert_eq!(points[0]["revenueCents"], 500_000); // day1: 300000 + 200000
    assert_eq!(points[1]["revenueCents"], 100_000); // day2 line total, pre-invoice-discount
    assert_eq!(points[1]["cogsCents"], 60_000);
    assert_eq!(body["data"]["totals"]["revenueCents"], 600_000);

    // Payment methods: every invoice was cash.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/payment-methods?preset=custom&from=2021-06-15&to=2021-06-16",
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
        "/api/reports/analytics/top-customers?preset=custom&from=2021-06-15&to=2021-06-16",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let customers = body["data"]["customers"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(customers.len(), 1);
    assert_eq!(customers[0]["isWalkIn"], true);
    assert_eq!(customers[0]["invoiceCount"], 3);
    // retail line revenue (300000 + 100000 + 200000) − cogs (180000 + 60000 + 40000)
    assert_eq!(customers[0]["grossProfitCents"], 320_000);

    // Cashier performance: one cashier, both invoices.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/cashier-performance?preset=custom&from=2021-06-15&to=2021-06-16",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let cashiers = body["data"]["cashiers"]
        .as_array()
        .expect("the value is an array");
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
        "/api/reports/analytics/sales-by-category?preset=custom&from=2021-06-15&to=2021-06-16",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["uncategorised"]["revenueCents"], 400_000); // 300000 + 100000
    assert_eq!(body["data"]["totalRevenueCents"], 600_000);
    let rows = body["data"]["rows"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["categoryName"], "AX Widgets");
    assert_eq!(rows[0]["revenueCents"], 200_000);

    // Sales patterns: 168 zero-filled cells, both sales on the same weekday cell counted.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/sales-patterns?preset=custom&from=2021-06-15&to=2021-06-16",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["cells"]
            .as_array()
            .expect("the value is an array")
            .len(),
        168
    );
    let total_pattern_invoices: u64 = body["data"]["byWeekday"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|b| {
            b["invoiceCount"]
                .as_u64()
                .expect("the value is an unsigned integer")
        })
        .sum();
    assert_eq!(total_pattern_invoices, 3);

    // Discounts: one invoice had a 10000 order-level discount.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/reports/analytics/discounts?preset=custom&from=2021-06-15&to=2021-06-16",
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
        .expect("the request is well formed");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
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
        .expect("the awaited step succeeds");
    assert!(bytes.starts_with(b"%PDF"), "response body should be a PDF");
}
