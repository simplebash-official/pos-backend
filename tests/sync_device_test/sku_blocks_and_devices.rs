//! Sku blocks and devices tests.

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};

use serde_json::{Value, json};

use simplebash_pos_backend::domain::sequences::ReserveSequenceRequest;

use tower::ServiceExt;

use crate::helpers::{Device, apply, as_pulled, create_cashier, login_status, table_rows};

/// Two enabled devices with empty outboxes.
async fn linked_pair() -> (Device, Device) {
    let a = Device::new().await;
    let b = Device::new().await;
    for dev in [&a, &b] {
        dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
        dev.sync(
            "POST",
            "/api/sync/outbox/ack",
            Some(json!({ "upToSeq": 1_000_000 })),
        )
        .await;
    }
    (a, b)
}

async fn hand_over(from: &Device, to: &Device) -> Value {
    let changes = as_pulled(&from.drain_outbox().await);
    from.sync(
        "POST",
        "/api/sync/outbox/ack",
        Some(json!({ "upToSeq": 1_000_000 })),
    )
    .await;
    to.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "incremental", "changes": changes })),
    )
    .await
}

// ---------------------------------------------------------------------------
// Quotations
// ---------------------------------------------------------------------------

fn quotation_body(customer: &str) -> Value {
    let until = (chrono::Utc::now() + chrono::Duration::days(30))
        .format("%Y-%m-%d")
        .to_string();
    json!({
        "customer": { "customerName": customer, "customerPhone": "0771234567" },
        "items": [{ "name": "Screen replacement", "unitPriceCents": 10000, "quantity": 2, "discountCents": 500 }],
        "pricingAdjustments": { "discountType": "percentage", "discountValue": 10.0 },
        "validUntil": until, "notes": "Excludes courier", "terms": "50% advance", "preparedBy": "Kamal"
    })
}

async fn create_quotation(dev: &Device, customer: &str) -> String {
    let created = dev
        .api("POST", "/api/quotations", Some(quotation_body(customer)))
        .await;
    created["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string()
}

async fn cancel_quotation(dev: &Device, key: &str) {
    dev.api("POST", &format!("/api/quotations/{key}/cancel"), None)
        .await;
}

/// The quotation entries of an outbox drain, as the `changes` of an apply request.
fn quotation_changes(items: &[Value]) -> Vec<Value> {
    let only: Vec<Value> = items
        .iter()
        .filter(|item| item["record"]["resource"] == "quotations")
        .cloned()
        .collect();
    as_pulled(&only)
}

async fn quotation_status(dev: &Device, key: &str) -> String {
    let found = dev
        .api("GET", &format!("/api/quotations/{key}"), None)
        .await;
    found["data"]["status"]
        .as_str()
        .expect("the value is a string")
        .to_string()
}

async fn enabled_pair() -> (Device, Device) {
    let (a, b) = (Device::new().await, Device::new().await);
    a.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    b.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    for dev in [&a, &b] {
        dev.sync(
            "POST",
            "/api/sync/outbox/ack",
            Some(json!({ "upToSeq": 1_000_000 })),
        )
        .await;
    }
    (a, b)
}

async fn ack_all(dev: &Device) {
    dev.sync(
        "POST",
        "/api/sync/outbox/ack",
        Some(json!({ "upToSeq": 1_000_000 })),
    )
    .await;
}

/// `GET /api/sync/sku-prefixes` is how the desktop agent discovers which
/// `sku:<prefix>` block families to keep topped up - it has no other way to
/// know which category+subcategory pairs exist locally.
#[tokio::test]
async fn sku_prefixes_lists_the_distinct_local_catalog_prefixes() {
    let dev = Device::new().await;
    // A fresh device already has the built-in Uncategorized / General category.
    assert_eq!(
        dev.sync("GET", "/api/sync/sku-prefixes", None).await["prefixes"],
        json!(["UNC-GEN"])
    );

    let mut category_keys = std::collections::HashMap::new();
    for (category, subcategory) in [
        ("Phone Repairs", "Screens"),
        ("Phone Repairs", "Batteries"),
        ("Electronics", "Cables"),
    ] {
        let category_key = match category_keys.get(category) {
            Some(key) => key,
            None => {
                let cat = dev
                    .api(
                        "POST",
                        "/api/inventory/categories",
                        Some(json!({ "name": category, "icon": "x", "color": "red" })),
                    )
                    .await;
                let key = cat["data"]["key"]
                    .as_str()
                    .expect("the value is a string")
                    .to_string();
                category_keys.insert(category.to_string(), key);
                category_keys.get(category).expect("the key is present")
            }
        };
        dev.api(
            "POST",
            &format!("/api/inventory/categories/{category_key}/subcategories"),
            Some(json!({ "name": subcategory })),
        )
        .await;
    }

    let mut prefixes: Vec<String> =
        dev.sync("GET", "/api/sync/sku-prefixes", None).await["prefixes"]
            .as_array()
            .expect("the value is an array")
            .iter()
            .map(|v| v.as_str().expect("the value is a string").to_string())
            .collect();
    prefixes.sort();
    assert_eq!(prefixes, vec!["ELE-CAB", "PHO-BAT", "PHO-SCR", "UNC-GEN"]);
}

#[tokio::test]
async fn backups_never_carry_sync_bookkeeping() {
    let dev = Device::new().await;
    dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    dev.api(
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "Backed up", "icon": "x", "color": "red" })),
    )
    .await;

    let export = dev.api("POST", "/api/backup/export", Some(json!({}))).await;
    let tables = export["data"]["tables"]
        .as_object()
        .or_else(|| export["data"].as_object())
        .expect("export lists tables");
    assert!(tables.contains_key("categories"));
    assert!(
        tables.keys().all(|t| !t.starts_with("sync_")),
        "sync tables leaked into the backup: {:?}",
        tables.keys().collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_cashier_created_on_one_device_can_log_in_on_another() {
    let (a, b) = linked_pair().await;
    let (id, key) = create_cashier(&a, "cash1", "cashier-pass-1").await;

    let changes = as_pulled(&a.drain_outbox().await);
    let record = changes
        .iter()
        .find(|c| c["resource"] == "users" && c["key"] == key.as_str())
        .expect("the user is captured");
    assert!(
        record["payload"]["passwordHash"]
            .as_str()
            .expect("the value is a string")
            .starts_with("$argon2"),
        "the sync record carries the hash so the login works elsewhere"
    );
    b.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "incremental", "changes": changes })),
    )
    .await;

    // Same row on both devices: same legacy id (login tokens carry it) and same hash.
    let row = |dev: &Device| {
        let pool = dev.pool().clone();
        let key = key.clone();
        async move {
            sqlx::query_as::<_, (String, String, String)>(
                "SELECT id, password_hash, role FROM users WHERE key = ?",
            )
            .bind(key)
            .fetch_one(&pool)
            .await
            .expect("the awaited step succeeds")
        }
    };
    let (row_a, row_b) = (row(&a).await, row(&b).await);
    assert_eq!(row_a, row_b);
    assert_eq!(row_b.0, id);

    assert_eq!(
        login_status(&b, "cash1", "cashier-pass-1").await,
        StatusCode::OK
    );
    assert_eq!(
        login_status(&b, "cash1", "wrong-password").await,
        StatusCode::UNAUTHORIZED
    );
    // Applying created no echo: B has nothing to send back.
    assert_eq!(b.pending_out().await, 0);
}

#[tokio::test]
async fn deactivating_and_deleting_a_cashier_propagates() {
    let (a, b) = linked_pair().await;
    let (id, key) = create_cashier(&a, "cash2", "cashier-pass-2").await;
    hand_over(&a, &b).await;
    assert_eq!(
        login_status(&b, "cash2", "cashier-pass-2").await,
        StatusCode::OK
    );

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    a.api(
        "PATCH",
        &format!("/api/users/{id}"),
        Some(json!({ "isActive": false })),
    )
    .await;
    hand_over(&a, &b).await;
    let active: bool = sqlx::query_scalar("SELECT is_active FROM users WHERE key = ?")
        .bind(&key)
        .fetch_one(b.pool())
        .await
        .expect("the awaited step succeeds");
    assert!(!active, "the deactivation reached B");
    assert_ne!(
        login_status(&b, "cash2", "cashier-pass-2").await,
        StatusCode::OK,
        "a deactivated cashier cannot log in"
    );

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    a.api("DELETE", &format!("/api/users/{id}"), None).await;
    hand_over(&a, &b).await;
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE key = ?")
        .bind(&key)
        .fetch_one(b.pool())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(remaining, 0, "the delete removed the login on B");
    assert_ne!(
        login_status(&b, "cash2", "cashier-pass-2").await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn the_password_hash_never_lands_in_a_conflict_record() {
    let (a, b) = linked_pair().await;
    let (id, key) = create_cashier(&a, "cash3", "cashier-pass-3").await;
    hand_over(&a, &b).await;

    // Both devices rename the cashier; B's edit is later, so A's edit loses on B.
    a.api(
        "PATCH",
        &format!("/api/users/{id}"),
        Some(json!({ "name": "Cashier (A)" })),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    b.api(
        "PATCH",
        &format!("/api/users/{id}"),
        Some(json!({ "name": "Cashier (B)" })),
    )
    .await;
    let from_a = as_pulled(&a.drain_outbox().await);
    let applied = b
        .sync(
            "POST",
            "/api/sync/apply",
            Some(json!({ "mode": "incremental", "changes": from_a })),
        )
        .await;
    assert_eq!(applied["applied"], 0, "{applied}");

    let conflicts = b.sync("GET", "/api/sync/conflicts", None).await;
    let conflict = conflicts
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|c| c["kind"] == "LWW_LOSER" && c["entityKey"] == key.as_str())
        .expect("the losing user edit is recorded");
    assert_eq!(conflict["detail"]["losingPayload"]["name"], "Cashier (A)");
    assert!(
        conflict["detail"]["losingPayload"]
            .get("passwordHash")
            .is_none(),
        "{conflict}"
    );
    let stored: String = sqlx::query_scalar("SELECT detail FROM sync_conflicts")
        .fetch_one(b.pool())
        .await
        .expect("the awaited step succeeds");
    assert!(
        !stored.contains("argon2") && !stored.contains("passwordHash"),
        "hash leaked into sync_conflicts: {stored}"
    );
}

#[tokio::test]
async fn two_admins_from_two_devices_are_both_kept_and_reported() {
    let (a, b) = linked_pair().await;
    // Each device made its own shop owner offline.
    let mk = |dev: &Device, username: &'static str| {
        let dev_admin = dev.admin.clone();
        let router = dev.app.router.clone();
        async move {
            let request = Request::builder()
                .method("POST")
                .uri("/api/users")
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, format!("Bearer {dev_admin}"))
                .body(Body::from(json!({ "name": "Owner", "username": username, "password": "owner-pass-12", "role": "staff" }).to_string()))
                .expect("the request is well formed");
            let response = router
                .oneshot(request)
                .await
                .expect("the awaited step succeeds");
            assert!(response.status().is_success());
        }
    };
    mk(&a, "owner-a").await;
    mk(&b, "owner-b").await;
    // Promote both to admin directly (the service's one-admin rule guards the API path only).
    for dev in [&a, &b] {
        sqlx::query("UPDATE users SET role = 'admin'")
            .execute(dev.pool())
            .await
            .expect("the awaited step succeeds");
    }
    hand_over(&a, &b).await;

    let admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE role = 'admin' AND deleted_at IS NULL",
    )
    .fetch_one(b.pool())
    .await
    .expect("the awaited step succeeds");
    assert_eq!(admins, 2, "neither owner was dropped");
    let conflicts = b.sync("GET", "/api/sync/conflicts", None).await;
    let raised = conflicts
        .as_array()
        .expect("the value is an array")
        .iter()
        .filter(|c| c["detail"]["reason"] == "MULTIPLE_ADMINS")
        .count();
    assert_eq!(raised, 1, "the shop owner is told once: {conflicts}");
}

#[test]
fn logging_redaction_masks_the_password_hash_under_both_spellings() {
    use simplebash_pos_backend::core::logging::redact::redact_value;
    let mut value = json!({ "changes": [{ "payload": { "passwordHash": "$argon2id$v=19$secret", "name": "A" } }], "password_hash": "$argon2id$other" });
    redact_value(&mut value);
    let text = value.to_string();
    assert!(!text.contains("argon2"), "{text}");
    assert!(text.contains("\"name\":\"A\""));
}

#[tokio::test]
async fn a_quotation_and_its_cancellation_reach_another_device_and_back() {
    let (a, b) = enabled_pair().await;

    let first = create_quotation(&a, "Nimal").await;
    let second = create_quotation(&a, "Rita").await;
    cancel_quotation(&a, &second).await;

    let outbox = a.drain_outbox().await;
    let changes = quotation_changes(&outbox);
    assert_eq!(
        changes.len(),
        2,
        "the second quotation's create and cancel coalesce into one entry: {changes:?}"
    );
    let cancelled = changes
        .iter()
        .find(|c| c["key"] == second.as_str())
        .expect("the cancelled quotation is queued");
    assert_eq!(cancelled["payload"]["status"], "cancelled");
    assert_eq!(cancelled["version"], 2);
    // Everything a device needs to reprint travels; the date-derived flag does not.
    assert!(cancelled["payload"]["shopProfileSnapshot"].is_object());
    assert!(cancelled["payload"].get("isExpired").is_none());

    apply(&b, changes).await;
    assert_eq!(
        table_rows(a.pool(), "quotations").await,
        table_rows(b.pool(), "quotations").await
    );
    assert_eq!(quotation_status(&b, &first).await, "active");
    assert_eq!(quotation_status(&b, &second).await, "cancelled");
    assert_eq!(b.pending_out().await, 0, "applied changes never echo");

    // The receiving device can list and print what it received.
    let listed = b.api("GET", "/api/quotations?status=cancelled", None).await;
    assert_eq!(listed["data"]["total"], 1);
    let (status, _) = b
        .call(
            "GET",
            &format!("/api/quotations/{first}/document?design=current"),
            None,
            Some(&b.admin.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // And the other way: B cancels the first one and A learns of it.
    cancel_quotation(&b, &first).await;
    let back = quotation_changes(&b.drain_outbox().await);
    assert_eq!(back.len(), 1);
    assert_eq!(back[0]["version"], 2);
    apply(&a, back).await;
    assert_eq!(quotation_status(&a, &first).await, "cancelled");
    assert_eq!(
        table_rows(a.pool(), "quotations").await,
        table_rows(b.pool(), "quotations").await
    );
}

#[tokio::test]
async fn applying_a_quotation_batch_twice_changes_nothing() {
    let (a, b) = enabled_pair().await;
    create_quotation(&a, "Nimal").await;
    let changes = quotation_changes(&a.drain_outbox().await);

    let first = apply(&b, changes.clone()).await;
    let before = table_rows(b.pool(), "quotations").await;
    let second = apply(&b, changes).await;
    assert_eq!(second["applied"], 0, "{second}");
    assert_eq!(second["duplicates"], first["applied"]);
    assert_eq!(before, table_rows(b.pool(), "quotations").await);
}

#[tokio::test]
async fn a_late_active_copy_never_undoes_a_cancellation() {
    let (a, b) = enabled_pair().await;
    let key = create_quotation(&a, "Nimal").await;
    let active = quotation_changes(&a.drain_outbox().await);
    ack_all(&a).await;
    cancel_quotation(&a, &key).await;
    let cancelled = quotation_changes(&a.drain_outbox().await);

    apply(&b, active.clone()).await;
    apply(&b, cancelled).await;
    assert_eq!(quotation_status(&b, &key).await, "cancelled");

    // The old "active" copy shows up again (a retry, or a slow device): ignored.
    apply(&b, active).await;
    assert_eq!(quotation_status(&b, &key).await, "cancelled");
}

#[tokio::test]
async fn cancelling_on_two_devices_at_once_converges_without_a_conflict() {
    let (a, b) = enabled_pair().await;
    let key = create_quotation(&a, "Nimal").await;
    apply(&b, quotation_changes(&a.drain_outbox().await)).await;
    ack_all(&a).await;

    cancel_quotation(&a, &key).await;
    cancel_quotation(&b, &key).await;
    let from_a = quotation_changes(&a.drain_outbox().await);
    let from_b = quotation_changes(&b.drain_outbox().await);

    apply(&a, from_b).await;
    apply(&b, from_a).await;

    assert_eq!(quotation_status(&a, &key).await, "cancelled");
    assert_eq!(quotation_status(&b, &key).await, "cancelled");
    assert_eq!(
        table_rows(a.pool(), "quotations").await,
        table_rows(b.pool(), "quotations").await
    );
}

#[tokio::test]
async fn linked_devices_draw_quotation_numbers_from_blocks_then_a_unique_fallback() {
    use simplebash_pos_backend::modules::sequences::service::reserve_sequence;
    let dev = Device::new().await;
    let single = || ReserveSequenceRequest {
        block_size: Some(1),
        device_id: None,
    };

    // Unlinked: the ordinary local counter, four digits.
    let plain = reserve_sequence(&dev.app.db_handle, "quotation".to_string(), single())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(
        (plain.prefix.as_str(), plain.start, plain.padding),
        ("QUO-", 1, 4)
    );

    dev.sync(
        "POST",
        "/api/sync/enable",
        Some(json!({ "deviceId": "dev_zzzz0000aaaa1111" })),
    )
    .await;
    dev.sync(
        "POST",
        "/api/sync/blocks",
        Some(json!({ "name": "quotation", "prefix": "QUO-", "padding": 4, "start": 700, "end": 701, "expiresAt": "2099-01-01T00:00:00Z" })),
    )
    .await;
    let n1 = reserve_sequence(&dev.app.db_handle, "quotation".to_string(), single())
        .await
        .expect("the awaited step succeeds");
    let n2 = reserve_sequence(&dev.app.db_handle, "quotations".to_string(), single())
        .await
        .expect("the awaited step succeeds");
    assert_eq!((n1.prefix.as_str(), n1.start), ("QUO-", 700));
    assert_eq!((n2.prefix.as_str(), n2.start), ("QUO-", 701));

    // Block used up: a per-device series that cannot collide with the cloud's numbers.
    let fallback = reserve_sequence(&dev.app.db_handle, "quotation".to_string(), single())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(fallback.prefix, "QUO-Dzzzz-");
    assert_eq!(fallback.start, 1);
}
