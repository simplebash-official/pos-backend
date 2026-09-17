mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use simplebash_pos_backend::{
    core::{
        config::Config,
        constants::{codes, roles},
        id::generate_id,
    },
    domain::users::Role,
    modules::customers::model::CustomerDocument,
};
use mongodb::bson::DateTime as BsonDateTime;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

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

fn staff_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    )
}

fn admin_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let app = common::spawn_app().await;

    let (status, body) = send(&app.router, "GET", "/api/customers", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["success"], false);

    let (status, body) = send(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": "Walk-in Customer",
            "primaryPhone": "0771122334"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn quick_create_customer_succeeds_with_minimal_fields() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let unique_phone = format!(
        "077{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            % 10_000_000
    );
    let create_payload = json!({
        "name": "Quick Cashier Customer",
        "primaryPhone": unique_phone
    });

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(create_payload),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["success"], true);

    let data = &body["data"];
    assert_eq!(data["name"], "Quick Cashier Customer");
    assert_eq!(data["primaryPhone"], unique_phone);
    assert_eq!(data["outstandingBalanceCents"], 0);
    assert_eq!(data["totalPurchasesCents"], 0);
    assert!(data["id"].as_str().is_some());
    assert!(
        data["key"].as_str().unwrap().starts_with("cust_"),
        "Key must start with cust_ prefix"
    );
    assert_eq!(data["contactPerson"], Value::Null);
    assert_eq!(data["email"], Value::Null);
    assert_eq!(data["address"], Value::Null);
    assert_eq!(data["notes"], Value::Null);
    assert_eq!(data["tags"], json!([]));
}

#[tokio::test]
async fn create_customer_with_full_profile_and_fetch_by_id_and_key() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let unique_suffix = Uuid::new_v4().to_string();
    let email = format!("customer_{unique_suffix}@example.com");
    let create_payload = json!({
        "name": "ABC Enterprises",
        "contactPerson": "Kavinda Fernando",
        "primaryPhone": "0114567890",
        "secondaryPhone": "0719876543",
        "email": email,
        "address": "Level 4, Millennium Tower",
        "tags": ["Corporate Account", "VIP"],
        "notes": "30-day credit period."
    });

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(create_payload),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    let created = &body["data"];
    let id = created["id"].as_str().unwrap();
    let key = created["key"].as_str().unwrap();

    assert_eq!(created["name"], "ABC Enterprises");
    assert_eq!(created["contactPerson"], "Kavinda Fernando");
    assert_eq!(created["email"], email);
    assert_eq!(created["tags"], json!(["Corporate Account", "VIP"]));

    // Fetch by Mongo ObjectId hex
    let (status, get_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(get_body["data"]["id"], id);
    assert_eq!(get_body["data"]["name"], "ABC Enterprises");

    // Fetch by prefixed Key
    let (status, get_key_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers/{key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(get_key_body["data"]["key"], key);
    assert_eq!(get_key_body["data"]["name"], "ABC Enterprises");
}

#[tokio::test]
async fn client_cannot_inject_financial_balances() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    // Attempting to send outstandingBalanceCents and totalPurchasesCents in POST payload
    let injected_payload = json!({
        "name": "Tampered Account",
        "primaryPhone": "0770001122",
        "outstandingBalanceCents": 9999999,
        "totalPurchasesCents": 8888888
    });

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(injected_payload),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let created = &body["data"];
    assert_eq!(
        created["outstandingBalanceCents"], 0,
        "Backend must ignore client financial totals"
    );
    assert_eq!(
        created["totalPurchasesCents"], 0,
        "Backend must ignore client financial totals"
    );
}

#[tokio::test]
async fn search_and_tag_filtering_works() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let prefix = Uuid::new_v4().to_string();
    let name1 = format!("Alpha {prefix} Corp");
    let name2 = format!("Beta {prefix} Retail");
    let tag = format!("Tag_{prefix}");

    send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": name1,
            "primaryPhone": "0771230001",
            "email": format!("alpha_{prefix}@test.com"),
            "tags": [tag.clone()]
        })),
        &token,
    )
    .await;

    send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": name2,
            "primaryPhone": "0771230002",
            "email": format!("beta_{prefix}@test.com"),
            "tags": ["OtherTag"]
        })),
        &token,
    )
    .await;

    // Search by name prefix
    let (status, search_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers?search={prefix}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let customers = search_body["data"]["customers"].as_array().unwrap();
    assert_eq!(customers.len(), 2);

    // Filter by tag
    let (status, tag_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers?tag={tag}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tag_customers = tag_body["data"]["customers"].as_array().unwrap();
    assert_eq!(tag_customers.len(), 1);
    assert_eq!(tag_customers[0]["name"], name1);
}

#[tokio::test]
async fn pagination_structure_matches_expected_format() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let batch_prefix = Uuid::new_v4().to_string();
    for i in 1..=5 {
        send_authed(
            &app.router,
            "POST",
            "/api/customers",
            Some(json!({
                "name": format!("Paging {batch_prefix} Customer {i}"),
                "primaryPhone": format!("077{i}000000")
            })),
            &token,
        )
        .await;
    }

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers?search={batch_prefix}&page=1&limit=2"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["total"], 5);
    assert_eq!(data["page"], 1);
    assert_eq!(data["limit"], 2);
    assert_eq!(data["totalPages"], 3);
    let items = data["customers"].as_array().unwrap();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn replace_and_update_customer_maintains_invariants() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (status, create_body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": "Original Customer",
            "primaryPhone": "0771111111",
            "notes": "Original note"
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = create_body["data"]["id"].as_str().unwrap();

    // PUT replacement
    let (status, put_body) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/customers/{id}"),
        Some(json!({
            "name": "Replaced Customer",
            "primaryPhone": "0772222222",
            "email": "replaced@example.com"
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(put_body["data"]["name"], "Replaced Customer");
    assert_eq!(put_body["data"]["primaryPhone"], "0772222222");
    assert_eq!(put_body["data"]["notes"], Value::Null); // Omitted notes are cleared in full PUT replacement

    // PATCH partial update
    let (status, patch_body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/customers/{id}"),
        Some(json!({
            "notes": "Added patch note"
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patch_body["data"]["name"], "Replaced Customer"); // Retained
    assert_eq!(patch_body["data"]["notes"], "Added patch note");
}

#[tokio::test]
async fn delete_customer_soft_deletes_and_excludes_from_lists() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (status, create_body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": "Deletable Customer",
            "primaryPhone": "0779998877"
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = create_body["data"]["id"].as_str().unwrap();

    // Delete single
    let (status, del_body) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/customers/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(del_body["success"], true);

    // Subsequent GET returns 404
    let (status, _) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_customer_with_outstanding_balance_is_blocked() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let (status, create_body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": "Indebted Customer",
            "primaryPhone": "0775554433"
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let key = create_body["data"]["key"].as_str().unwrap();
    let id = create_body["data"]["id"].as_str().unwrap();

    // Simulate an invoice ledger balance on the customer
    let update_res = app
        .db
        .collection::<mongodb::bson::Document>("customers")
        .update_one(
            mongodb::bson::doc! { "key": key },
            mongodb::bson::doc! {
                "$set": { "outstanding_balance_cents": 150000 }
            },
        )
        .await;
    assert!(update_res.is_ok());

    // Attempting delete should be rejected with 409 Conflict
    let (status, del_body) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/customers/{id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(del_body["code"], codes::CUSTOMER_HAS_OUTSTANDING_BALANCE);
}

#[tokio::test]
async fn batch_delete_customers_succeeds() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (status, c1) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({ "name": "Batch Cust 1", "primaryPhone": "0770011223" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id1 = c1["data"]["id"].as_str().unwrap().to_string();

    let (status, c2) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({ "name": "Batch Cust 2", "primaryPhone": "0770011224" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id2 = c2["data"]["id"].as_str().unwrap().to_string();

    let (status, batch_body) = send_authed(
        &app.router,
        "DELETE",
        "/api/customers/batch",
        Some(json!({
            "ids": [id1, id2, "invalid-id-or-nonexistent"]
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(batch_body["success"], true);
    assert_eq!(batch_body["data"]["deletedCount"], 2);
    assert_eq!(batch_body["message"], "2 Customers deleted successfully");
}

#[tokio::test]
async fn distinct_tags_autocomplete_endpoint_works() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let prefix = Uuid::new_v4().to_string();
    let tag_a = format!("A_Tag_{prefix}");
    let tag_b = format!("B_Tag_{prefix}");

    send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": "Tag Customer 1",
            "primaryPhone": "0771112221",
            "tags": [tag_a.clone(), tag_b.clone()]
        })),
        &token,
    )
    .await;

    let (status, body) = send_authed(&app.router, "GET", "/api/customers/tags", None, &token).await;

    assert_eq!(status, StatusCode::OK);
    let tags = body["data"]["tags"].as_array().unwrap();
    let tag_strings: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
    assert!(tag_strings.contains(&tag_a.as_str()));
    assert!(tag_strings.contains(&tag_b.as_str()));
}

#[tokio::test]
async fn customers_stats_endpoint_requires_auth() {
    let app = common::spawn_app().await;
    let (status, _) = send(&app.router, "GET", "/api/customers/stats", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Inserts a minimal `CustomerDocument` directly, so the stats test can
/// control `outstandingBalanceCents`/`deletedAt` precisely.
async fn seed_customer_with(db: &mongodb::Database, outstanding_balance_cents: i64, deleted: bool) {
    let key = generate_id("cus");
    let now = BsonDateTime::now();
    db.collection::<CustomerDocument>("customers")
        .insert_one(CustomerDocument {
            id: None,
            key,
            name: format!("Stats Test Customer {}", Uuid::new_v4()),
            contact_person: None,
            primary_phone: "0770000000".to_string(),
            secondary_phone: None,
            email: None,
            address: None,
            tags: vec![],
            notes: None,
            outstanding_balance_cents,
            total_purchases_cents: 0,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: if deleted { Some(now) } else { None },
            updated_by_device: None,
        })
        .await
        .expect("failed to seed customer");
}

#[tokio::test]
async fn customers_stats_endpoint_returns_totals_balance_and_debtor_count() {
    // `spawn_app`'s DB is shared across the suite, and `/customers/stats`
    // aggregates the WHOLE collection with no filter — connect an isolated
    // throwaway database instead, same as the billing/repairs/print-jobs
    // stats tests.
    let app = common::spawn_app().await;
    let db_name = format!("jtstats_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let db = simplebash_pos_backend::clients::mongo::connect(&app.config.mongodb_uri, &db_name)
        .await
        .expect("failed to connect to isolated stats test database");

    seed_customer_with(&db, 5000, false).await;
    seed_customer_with(&db, 3000, false).await;
    seed_customer_with(&db, 0, false).await;
    // Soft-deleted with a balance — must be excluded entirely.
    seed_customer_with(&db, 9000, true).await;

    let db_handle = simplebash_pos_backend::clients::db::Db::Mongo(db.clone());
    let stats = simplebash_pos_backend::modules::customers::service::get_customer_stats(&db_handle)
        .await
        .expect("get_customer_stats should succeed");

    assert_eq!(
        stats.total_customers, 3,
        "the soft-deleted customer must be excluded"
    );
    assert_eq!(stats.total_balance_due_cents, 5000 + 3000);
    assert_eq!(stats.active_debtors_count, 2);

    db.drop().await.ok();
}
