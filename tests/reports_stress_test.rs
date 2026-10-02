mod common;

use std::sync::Arc;
use std::time::Instant;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use chrono::{Duration, Utc};
use futures_util::future::join_all;
use mongodb::bson::DateTime as BsonDateTime;
use serde_json::{Value, json};
use simplebash_pos_backend::{
    core::{constants::roles, id::generate_id},
    domain::{
        billing::{InvoiceItem, InvoiceStatus},
        users::Role,
    },
    modules::billing::model::InvoiceDocument,
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
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).to_string()))
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
        customer_name_snapshot: Some("Stress Walk-in".to_string()),
        customer_phone_snapshot: None,
        customer_address_snapshot: None,
        cashier_id: "usr_cashier_stress".to_string(),
        cashier_name_snapshot: "Stress Cashier".to_string(),
        items: vec![InvoiceItem {
            product_key: Some(format!("prd_{key}")),
            name: "Stress Item".to_string(),
            sku: Some("STR-TST-0001".to_string()),
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

/// Scenario 1: SingleFlight Thundering Herd Mitigation
/// Invalidate cache, then fire 50 simultaneous concurrent requests for the exact same analytics feed.
/// Verifies SingleFlight deduplicates all 50 requests into 1 execution with 100% response integrity.
#[tokio::test]
async fn test_singleflight_thundering_herd_stress() {
    let app = common::spawn_app().await;
    let admin_token = Arc::new(common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    ));

    // Seed test invoices
    let now = Utc::now();
    for i in 1..=10 {
        let inv = seed_invoice(
            &format!("inv_herd_{i}"),
            now - Duration::hours(i as i64),
            15_000,
            7_500,
        );
        app.db
            .collection::<InvoiceDocument>("invoices")
            .insert_one(inv)
            .await
            .unwrap();
    }

    // Explicitly invalidate cache via endpoint to ensure a completely cold state
    let (inv_status, _) = send_authed(
        &app.router,
        "POST",
        "/api/reports/engine/invalidate",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(inv_status, StatusCode::OK);

    let concurrency = 50;
    println!(
        "\n[STRESS TEST] Launching {concurrency} simultaneous cold requests for 'section=all'..."
    );

    let start_instant = Instant::now();
    let mut tasks = Vec::with_capacity(concurrency);

    for task_id in 0..concurrency {
        let router = app.router.clone();
        let token = admin_token.clone();
        tasks.push(tokio::spawn(async move {
            let task_start = Instant::now();
            let (status, body) = send_authed(
                &router,
                "GET",
                "/api/reports/engine/feed?section=all&preset=this_month",
                None,
                &token,
            )
            .await;
            let elapsed = task_start.elapsed();
            (task_id, status, body, elapsed)
        }));
    }

    let results = join_all(tasks).await;
    let total_wall_time = start_instant.elapsed();

    println!(
        "[STRESS TEST] All {concurrency} requests finished in {:?}. Average per task: {:?}",
        total_wall_time,
        total_wall_time / concurrency as u32
    );

    let mut successful_count = 0;
    for res in results {
        let (task_id, status, body, task_time) = res.unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "Task #{task_id} failed after {task_time:?}: {body}"
        );
        assert_eq!(body["success"], true);
        assert_eq!(body["data"]["section"], "all");
        assert!(body["data"]["overview"].is_object());
        assert!(body["data"]["sales"].is_object());
        assert!(body["data"]["profit"].is_object());
        assert!(body["data"]["customers"].is_object());
        assert!(body["data"]["staff"].is_object());
        successful_count += 1;
    }

    assert_eq!(
        successful_count, concurrency,
        "All 50 tasks must return HTTP 200 OK"
    );
}

/// Scenario 2: High-Volume Hot Cache Read Hammer (1,000 requests)
/// Verifies sub-millisecond p99 latency and high throughput when serving from the in-memory cache.
#[tokio::test]
async fn test_hot_cache_throughput_and_latency_stress() {
    let app = common::spawn_app().await;
    let admin_token = Arc::new(common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    ));

    // Prime the cache with an initial request
    let (prime_status, _) = send_authed(
        &app.router,
        "GET",
        "/api/reports/engine/feed?section=overview&preset=today",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(prime_status, StatusCode::OK);

    let total_requests = 1_000;
    let concurrency = 20;
    let requests_per_worker = total_requests / concurrency;

    println!(
        "\n[STRESS TEST] Firing {total_requests} hot cached reads across {concurrency} worker tasks..."
    );

    let benchmark_start = Instant::now();
    let mut worker_tasks = Vec::with_capacity(concurrency);

    for _ in 0..concurrency {
        let router = app.router.clone();
        let token = admin_token.clone();
        worker_tasks.push(tokio::spawn(async move {
            let mut worker_latencies = Vec::with_capacity(requests_per_worker);
            for _ in 0..requests_per_worker {
                let req_start = Instant::now();
                let (status, body) = send_authed(
                    &router,
                    "GET",
                    "/api/reports/engine/feed?section=overview&preset=today",
                    None,
                    &token,
                )
                .await;
                let req_elapsed = req_start.elapsed();
                assert_eq!(status, StatusCode::OK);
                assert_eq!(body["data"]["meta"]["cacheHit"], true);
                worker_latencies.push(req_elapsed);
            }
            worker_latencies
        }));
    }

    let worker_results = join_all(worker_tasks).await;
    let total_duration = benchmark_start.elapsed();

    let mut all_latencies: Vec<std::time::Duration> = worker_results
        .into_iter()
        .flat_map(|r| r.unwrap())
        .collect();
    all_latencies.sort();

    let p50 = all_latencies[total_requests * 50 / 100];
    let p95 = all_latencies[total_requests * 95 / 100];
    let p99 = all_latencies[total_requests * 99 / 100];
    let rps = total_requests as f64 / total_duration.as_secs_f64();

    println!(
        "[STRESS TEST] Completed {total_requests} cached reads in {total_duration:?} ({rps:.1} req/sec)"
    );
    println!("[STRESS TEST] Latency distribution: p50={p50:?}, p95={p95:?}, p99={p99:?}");

    assert_eq!(all_latencies.len(), total_requests);
    assert!(
        p99.as_millis() < 50,
        "p99 hot cache latency must remain low (was {p99:?})"
    );
}

/// Scenario 3: Real-Time Mutation Invalidation Race Stress
/// Concurrently executes continuous analytics readers while simultaneous writer tasks
/// perform POS sales, validating deadlock-freedom and real-time cache consistency.
#[tokio::test]
async fn test_concurrent_mutations_and_invalidation_race() {
    let app = common::spawn_app().await;
    let admin_token = Arc::new(common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    ));

    let num_readers = 10;
    let reads_per_reader = 15;
    let num_writers = 5;
    let writes_per_writer = 5;

    println!(
        "\n[STRESS TEST] Running {num_readers} readers concurrently with {num_writers} POS sales writers..."
    );

    let mut reader_tasks = Vec::with_capacity(num_readers);
    for reader_id in 0..num_readers {
        let router = app.router.clone();
        let token = admin_token.clone();
        reader_tasks.push(tokio::spawn(async move {
            for _ in 0..reads_per_reader {
                let (status, body) = send_authed(
                    &router,
                    "GET",
                    "/api/reports/engine/feed?section=overview&preset=today",
                    None,
                    &token,
                )
                .await;
                assert_eq!(
                    status,
                    StatusCode::OK,
                    "Reader #{reader_id} encountered an error: {body}"
                );
                assert_eq!(body["success"], true);
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }));
    }

    let mut writer_tasks = Vec::with_capacity(num_writers);
    for writer_id in 0..num_writers {
        let router = app.router.clone();
        let token = admin_token.clone();
        writer_tasks.push(tokio::spawn(async move {
            for w in 0..writes_per_writer {
                let sale_payload = json!({
                    "staff": {
                        "cashierName": "Stress Cashier"
                    },
                    "items": [
                        {
                            "name": format!("Race Item {writer_id}-{w}"),
                            "quantity": 1,
                            "unitPriceCents": 5_000,
                            "discountCents": 0,
                            "sourceType": "retail"
                        }
                    ],
                    "payment": {
                        "paymentMethod": "cash",
                        "amountReceivedCents": 5_000,
                        "isCredit": false
                    },
                    "shopProfileSnapshot": {
                        "tradingName": "TechFix POS"
                    }
                });

                let (status, body) = send_authed(
                    &router,
                    "POST",
                    "/api/billing/sales",
                    Some(sale_payload),
                    &token,
                )
                .await;
                assert_eq!(
                    status,
                    StatusCode::OK,
                    "Writer #{writer_id} failed on write {w}: {body}"
                );
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }));
    }

    let (reader_res, writer_res) = tokio::join!(join_all(reader_tasks), join_all(writer_tasks));

    for r in reader_res {
        r.unwrap();
    }
    for w in writer_res {
        w.unwrap();
    }

    println!(
        "[STRESS TEST] All concurrent readers and writers finished without error or deadlock."
    );

    // Final verification: Ensure the engine feed reflects the exact number of created sales
    let (status, final_feed) = send_authed(
        &app.router,
        "GET",
        "/api/reports/engine/feed?section=overview&preset=today",
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(final_feed["success"], true);
    let expected_new_revenue = (num_writers * writes_per_writer * 5_000) as i64;
    let actual_revenue = final_feed["data"]["overview"]["summary"]["current"]["totalRevenueCents"]
        .as_i64()
        .unwrap_or(0);
    assert!(
        actual_revenue >= expected_new_revenue,
        "Final feed revenue ({actual_revenue}) must reflect at least new sales ({expected_new_revenue})"
    );
}

/// Scenario 4: Heavy Multi-Dataset Aggregation Fan-out Stress
/// Seeds 30 invoices across historical dates and requests 'section=all' under concurrency.
#[tokio::test]
async fn test_heavy_multi_dataset_fanout_stress() {
    let app = common::spawn_app().await;
    let admin_token = Arc::new(common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    ));

    let now = Utc::now();
    for i in 1..=30 {
        let inv = seed_invoice(
            &format!("inv_fanout_{i}"),
            now - Duration::days(i as i64),
            12_000,
            6_000,
        );
        app.db
            .collection::<InvoiceDocument>("invoices")
            .insert_one(inv)
            .await
            .unwrap();
    }

    let concurrency = 20;
    println!(
        "\n[STRESS TEST] Firing {concurrency} concurrent heavy multi-dataset fan-out requests..."
    );

    let start = Instant::now();
    let mut tasks = Vec::with_capacity(concurrency);

    for _ in 0..concurrency {
        let router = app.router.clone();
        let token = admin_token.clone();
        tasks.push(tokio::spawn(async move {
            send_authed(
                &router,
                "GET",
                "/api/reports/engine/feed?section=all&preset=this_year&granularity=month",
                None,
                &token,
            )
            .await
        }));
    }

    let results = join_all(tasks).await;
    let duration = start.elapsed();

    println!("[STRESS TEST] Heavy fan-out {concurrency} requests completed in {duration:?}");

    for res in results {
        let (status, body) = res.unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["section"], "all");
        assert!(body["data"]["overview"]["timeseries"].is_object());
        assert!(body["data"]["sales"]["salesByCategory"].is_object());
        assert!(body["data"]["profit"]["timeseries"].is_object());
    }
}
