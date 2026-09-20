mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use simplebash_pos_backend::{
    core::{constants::roles, id::generate_id},
    domain::users::Role,
    modules::print_jobs::model::PrintJobDocument,
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

fn staff_token(config: &simplebash_pos_backend::core::config::Config) -> String {
    common::mint_token(
        config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    )
}

fn unprivileged_token(config: &simplebash_pos_backend::core::config::Config) -> String {
    common::mint_token(config, Some(Role::Staff), &[])
}

fn sample_print_job_payload() -> Value {
    json!({
        "customer": { "customerName": "Nadeesha Perera" },
        "jobType": "t-shirt",
        "quantity": 12,
        "estimatedCostCents": 240000,
    })
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let app = common::spawn_app().await;

    let (status, _) = send(&app.router, "GET", "/api/print-jobs", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(sample_print_job_payload()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_without_print_jobs_write_permission_is_rejected() {
    let app = common::spawn_app().await;
    let token = unprivileged_token(&app.config);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(sample_print_job_payload()),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Body: {body}");
    assert_eq!(body["code"], "PERMISSION_DENIED");
}

#[tokio::test]
async fn create_print_job_reserves_ticket_number_and_defaults_status_to_received() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(sample_print_job_payload()),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let data = &body["data"];
    assert_eq!(data["status"], "received");
    assert_eq!(data["jobType"], "t-shirt");
    assert_eq!(data["quantity"], 12);
    assert!(
        data["ticketNumber"].as_str().unwrap().starts_with("PRN-"),
        "ticket number must start with PRN-, got {}",
        data["ticketNumber"]
    );
    assert!(data["key"].as_str().unwrap().starts_with("prn_"));
    assert_eq!(data["isOverdue"], false);
}

#[tokio::test]
async fn print_job_promised_ready_at_round_trips_and_drives_is_overdue() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = sample_print_job_payload();
    payload["promisedReadyAt"] = json!("2000-01-01");
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["promisedReadyAt"], "2000-01-01");
    assert_eq!(body["data"]["isOverdue"], true);
    let key = body["data"]["key"].as_str().unwrap().to_string();

    let (_, body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/print-jobs/{key}"),
        Some(json!({ "promisedReadyAt": "2999-12-31" })),
        &token,
    )
    .await;
    assert_eq!(body["data"]["isOverdue"], false);
}

#[tokio::test]
async fn print_job_rejects_malformed_promised_ready_at() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = sample_print_job_payload();
    payload["promisedReadyAt"] = json!("2026/09/15");
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_print_job_rejects_invalid_job_type_and_zero_quantity() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = sample_print_job_payload();
    payload["jobType"] = json!("not-a-real-type");
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Body: {body}");
    assert_eq!(body["code"], "INVALID_JOB_TYPE");

    let mut payload = sample_print_job_payload();
    payload["quantity"] = json!(0);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Body: {body}");
}

#[tokio::test]
async fn get_print_job_by_id_and_key_and_update_status() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (_, created) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(sample_print_job_payload()),
        &token,
    )
    .await;
    let id = created["data"]["id"].as_str().unwrap();
    let key = created["data"]["key"].as_str().unwrap();

    let (status, by_id) = send_authed(
        &app.router,
        "GET",
        &format!("/api/print-jobs/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(by_id["data"]["key"], key);

    let (status, by_key) = send_authed(
        &app.router,
        "GET",
        &format!("/api/print-jobs/{key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(by_key["data"]["id"], id);

    let (status, updated) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/print-jobs/{key}"),
        Some(json!({ "status": "ready" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {updated}");
    assert_eq!(updated["data"]["status"], "ready");
    assert_eq!(
        updated["data"]["jobType"], "t-shirt",
        "unspecified fields must be left unchanged"
    );
}

#[tokio::test]
async fn list_print_jobs_filters_by_search_and_status() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let unique = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let mut payload = sample_print_job_payload();
    payload["customer"]["customerName"] = json!(format!("Findable-{unique}"));
    send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(payload),
        &token,
    )
    .await;

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/print-jobs?search=Findable-{unique}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let print_jobs = body["data"]["printJobs"].as_array().unwrap();
    assert_eq!(print_jobs.len(), 1, "Body: {body}");
    assert_eq!(print_jobs[0]["customerName"], format!("Findable-{unique}"));

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/print-jobs?status=cancelled",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for job in body["data"]["printJobs"].as_array().unwrap() {
        assert_eq!(job["status"], "cancelled");
    }
}

#[tokio::test]
async fn list_print_jobs_filters_by_date_preset() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let unique = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let name = format!("DatePreset-{unique}");
    let now = BsonDateTime::now();
    let yesterday = BsonDateTime::from_chrono(now.to_chrono() - chrono::Duration::days(1));
    seed_print_job_with_named(&app.db, &name, "received", 1000, now).await;
    seed_print_job_with_named(&app.db, &name, "received", 2000, yesterday).await;

    // No datePreset — search alone finds both.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/print-jobs?search={name}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["printJobs"].as_array().unwrap().len(), 2);

    // datePreset=today narrows to just the one created "now".
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/print-jobs?search={name}&datePreset=today"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let print_jobs = body["data"]["printJobs"].as_array().unwrap();
    assert_eq!(print_jobs.len(), 1, "Body: {body}");
    assert_eq!(print_jobs[0]["estimatedCostCents"], 1000);
}

#[tokio::test]
async fn delete_print_job_soft_deletes_and_excludes_from_lists() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (_, created) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(sample_print_job_payload()),
        &token,
    )
    .await;
    let key = created["data"]["key"].as_str().unwrap();

    let (status, _) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/print-jobs/{key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/print-jobs/{key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Body: {body}");
    assert_eq!(body["code"], "PRINT_JOB_NOT_FOUND");
}

async fn seed_customer(app: &common::TestApp, phone_suffix: &str) -> (String, String) {
    let token = staff_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": "Real Customer Name",
            "primaryPhone": format!("072{phone_suffix}"),
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    (
        body["data"]["key"].as_str().unwrap().to_string(),
        body["data"]["primaryPhone"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn create_print_job_with_customer_key_resolves_name_and_phone_from_customer_record() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (customer_key, customer_phone) = seed_customer(&app, "9998881").await;

    // customerName/customerPhone here are deliberately wrong — a valid
    // customerKey must override both from the linked customer record.
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(json!({
            "customer": {
                "customerKey": customer_key,
                "customerName": "Wrong Name",
                "customerPhone": "0009999999",
            },
            "jobType": "t-shirt",
            "quantity": 12,
            "estimatedCostCents": 240000,
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let data = &body["data"];
    assert_eq!(data["customerName"], "Real Customer Name");
    assert_eq!(data["customerPhone"], customer_phone);
}

#[tokio::test]
async fn update_print_job_with_customer_key_resolves_name_and_phone_from_customer_record() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_, created) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(sample_print_job_payload()),
        &token,
    )
    .await;
    let key = created["data"]["key"].as_str().unwrap();

    let (customer_key, customer_phone) = seed_customer(&app, "9998882").await;

    let (status, updated) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/print-jobs/{key}"),
        Some(json!({
            "customer": {
                "customerKey": customer_key,
                "customerName": "Wrong Name",
                "customerPhone": "0009999999",
            },
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Body: {updated}");
    assert_eq!(updated["data"]["customerName"], "Real Customer Name");
    assert_eq!(updated["data"]["customerPhone"], customer_phone);
}

/// Inserts a minimal `PrintJobDocument` directly, bypassing
/// `POST /print-jobs`, so the stats test can control `created_at`/`status`/
/// `estimated_cost_cents` precisely.
async fn seed_print_job_with_named(
    db: &mongodb::Database,
    customer_name: &str,
    status: &str,
    estimated_cost_cents: i64,
    created_at: BsonDateTime,
) -> String {
    let key = generate_id("prj");
    db.collection::<PrintJobDocument>("print_jobs")
        .insert_one(PrintJobDocument {
            id: None,
            key: key.clone(),
            ticket_number: format!("PRN-{}", &key[4..]),
            customer_key: None,
            customer_name: customer_name.to_string(),
            customer_phone: Some("0770000000".to_string()),
            job_type: "mug".to_string(),
            quantity: 1,
            promised_ready_at: None,
            status: status.to_string(),
            estimated_cost_cents,
            material_cost_cents: None,
            assigned_employee_id: None,
            assigned_employee_name: None,
            split_type: None,
            split_value: None,
            version: 1,
            created_at,
            updated_at: created_at,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .unwrap();
    key
}

async fn seed_print_job_with(
    db: &mongodb::Database,
    status: &str,
    estimated_cost_cents: i64,
    created_at: BsonDateTime,
) -> String {
    seed_print_job_with_named(
        db,
        "Stats Test Customer",
        status,
        estimated_cost_cents,
        created_at,
    )
    .await
}

#[tokio::test]
async fn print_jobs_stats_endpoint_requires_auth() {
    let app = common::spawn_app().await;
    let (status, _) = send(&app.router, "GET", "/api/print-jobs/stats", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn print_jobs_stats_endpoint_returns_today_job_count_revenue_pending_count_and_avg_value() {
    // `spawn_app`'s DB is shared across the suite, which every other test
    // tolerates by scoping its own queries — not possible here since the
    // stats endpoint aggregates the WHOLE collection with no filter. So
    // this test connects its own throwaway database, seeds only into that,
    // and calls the service function directly (bypassing HTTP/router
    // entirely) to get exact, uncontaminated numbers.
    let app = common::spawn_app().await;
    // Atlas caps database names at 38 bytes.
    let db_name = format!(
        "jtstats_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..24]
    );
    let db = simplebash_pos_backend::clients::mongo::connect(&app.config.mongodb_uri, &db_name)
        .await
        .expect("failed to connect to isolated stats test database");

    let now = BsonDateTime::now();
    let yesterday = BsonDateTime::from_chrono(now.to_chrono() - chrono::Duration::days(1));

    // Today: 2 open jobs (received, in_repair) + 1 delivered (still counts
    // toward today's job count/revenue, but not pending).
    seed_print_job_with(&db, "received", 5000, now).await;
    seed_print_job_with(&db, "in_repair", 4000, now).await;
    seed_print_job_with(&db, "delivered", 3000, now).await;
    // Backdated open job — must NOT count toward today's job
    // count/revenue, but DOES count toward the all-time pending count.
    seed_print_job_with(&db, "diagnosing", 2000, yesterday).await;
    // Backdated cancelled job — excluded from pending entirely.
    seed_print_job_with(&db, "cancelled", 1000, yesterday).await;

    let db_handle = simplebash_pos_backend::clients::db::Db::from_mongo(db.clone());
    let stats = simplebash_pos_backend::modules::print_jobs::service::get_print_job_stats(&db_handle)
        .await
        .expect("get_print_job_stats should succeed");

    // today_job_count includes all 3 jobs created "now", regardless of status.
    assert_eq!(stats.today_job_count, 3);
    assert_eq!(stats.today_revenue_cents, 5000 + 4000 + 3000);
    // pending_job_count = all non-delivered/cancelled jobs, any date:
    // "received" + "in_repair" (today) + "diagnosing" (yesterday) = 3.
    assert_eq!(stats.pending_job_count, 3);
    assert_eq!(stats.avg_job_value_cents, (5000 + 4000 + 3000) / 3);

    db.drop().await.ok();
}

// ============================================================================
// Employee assignment FK resolution
// ============================================================================

fn admin_token(config: &simplebash_pos_backend::core::config::Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}

async fn create_employee_via_api(router: &axum::Router, token: &str) -> Value {
    let (status, body) = send_authed(
        router,
        "POST",
        "/api/employees",
        Some(json!({
            "name": format!("Test Printer {}", uuid::Uuid::new_v4()),
            "phone": "0771234567",
            "role": "printer",
            "defaultSplitType": "fixed",
            "defaultSplitValue": 500.0,
        })),
        token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["data"].clone()
}

#[tokio::test]
async fn create_print_job_with_valid_assigned_employee_id_resolves_name() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let employee = create_employee_via_api(&app.router, &admin).await;
    let employee_key = employee["key"].as_str().unwrap();

    let mut payload = sample_print_job_payload();
    payload["assignment"] = json!({
        "assignedEmployeeId": employee_key,
        "assignedEmployeeName": "Someone Else Entirely",
        "splitType": "fixed",
        "splitValue": 500.0,
    });

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(payload),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["assignedEmployeeId"], employee_key);
    assert_eq!(body["data"]["assignedEmployeeName"], employee["name"]);
}

#[tokio::test]
async fn create_print_job_with_unknown_assigned_employee_id_is_not_found() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);

    let mut payload = sample_print_job_payload();
    payload["assignment"] = json!({ "assignedEmployeeId": "emp_does_not_exist" });

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/print-jobs",
        Some(payload),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "EMPLOYEE_NOT_FOUND");
}
