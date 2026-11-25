//! The money ledgers and shop rules (cash categories, payees, incomes, openings, attachments and
//! the rest): each of them must leave a device and arrive whole on another.

use axum::http::StatusCode;
use serde_json::json;
use simplebash_pos_backend::modules::sync::migration::migrate_sqlite;

use crate::common::{LEDGER_RESOURCES, create_ledger_records};
use crate::helpers::{Device, apply, as_pulled, table_rows};

/// Columns that are stored as 0/1 on a device and must travel as JSON booleans.
const BOOLEAN_FIELDS: &[&str] = &["active", "pinned", "isSystem", "requiresReceipt"];

#[tokio::test]
async fn every_ledger_record_reaches_another_device() {
    let a = Device::new().await;
    let b = Device::new().await;
    a.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    create_ledger_records(&a.app.router, &a.admin).await;

    let queued = a.pending_out().await;
    let items = a.drain_outbox().await;
    assert_eq!(
        i64::try_from(items.len()).expect("the count fits"),
        queued,
        "every queued row is offered for upload"
    );
    for item in &items {
        let payload = &item["record"]["payload"];
        for field in BOOLEAN_FIELDS {
            if let Some(value) = payload.get(*field) {
                assert!(value.is_boolean(), "{field} travels as a boolean: {item}");
            }
        }
    }

    apply(&b, as_pulled(&items)).await;
    for (_, table) in LEDGER_RESOURCES {
        let sent = table_rows(a.pool(), table).await;
        assert!(!sent.is_empty(), "the test creates a row in {table}");
        assert_eq!(
            sent,
            table_rows(b.pool(), table).await,
            "table {table} differs"
        );
    }
}

/// The shop's built-in cash categories fill more than one upload page on their own; work queued
/// behind them must still go up.
#[tokio::test]
async fn a_full_page_of_ledger_rows_does_not_hold_back_later_work() {
    let dev = Device::new().await;
    dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    dev.api(
        "POST",
        "/api/customers",
        Some(json!({ "name": "Jane Doe", "primaryPhone": "0771234567" })),
    )
    .await;

    // The desktop agent's page size and stop rule.
    let mut after = 0i64;
    let mut resources = Vec::new();
    loop {
        let page = dev
            .sync(
                "GET",
                &format!("/api/sync/outbox?after={after}&limit=25"),
                None,
            )
            .await;
        let items = page["items"].as_array().expect("items is an array");
        if items.is_empty() {
            break;
        }
        resources.extend(items.iter().map(|item| item["record"]["resource"].clone()));
        after = page["lastSeq"].as_i64().expect("lastSeq is an integer");
    }
    assert!(
        resources.contains(&json!("customers")),
        "the customer was uploaded: {resources:?}"
    );
    assert_eq!(
        i64::try_from(resources.len()).expect("the count fits"),
        dev.pending_out().await
    );
}

/// A queued row the backend cannot turn into a record must stop the upload loudly: skipping it
/// would let the next acknowledgement delete it unsent.
#[tokio::test]
async fn a_queued_row_that_cannot_be_read_fails_the_page_and_stays_queued() {
    let dev = Device::new().await;
    dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    sqlx::query(
        "INSERT INTO sync_outbox (resource, key, op, enqueued_at) \
         VALUES ('unknownThings', 'x', 'upsert', '2026-01-01T00:00:00Z')",
    )
    .execute(dev.pool())
    .await
    .expect("the row is queued");
    let queued = dev.pending_out().await;

    let (status, body) = dev
        .call(
            "GET",
            "/api/sync/outbox?after=0&limit=500",
            None,
            Some(&dev.agent),
        )
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(dev.pending_out().await, queued);
}

/// A database from before the catch-up has no marker column.
async fn forget_the_requeue(dev: &Device) {
    sqlx::query("ALTER TABLE sync_state DROP COLUMN ledger_requeued_at")
        .execute(dev.pool())
        .await
        .expect("the marker is dropped");
}

async fn acknowledge_everything(dev: &Device) {
    let items = dev.drain_outbox().await;
    let last = items.last().expect("something is queued")["seq"].clone();
    dev.sync(
        "POST",
        "/api/sync/outbox/ack",
        Some(json!({ "upToSeq": last })),
    )
    .await;
    assert_eq!(dev.pending_out().await, 0);
}

#[tokio::test]
async fn a_syncing_device_queues_its_ledger_rows_again_exactly_once() {
    let dev = Device::new().await;
    dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    create_ledger_records(&dev.app.router, &dev.admin).await;
    acknowledge_everything(&dev).await;
    forget_the_requeue(&dev).await;

    migrate_sqlite(dev.pool()).await.expect("the catch-up runs");

    let mut rows = 0;
    for (resource, table) in LEDGER_RESOURCES {
        // Repair services always could be read, so none of theirs were dropped.
        if resource != "repairServices" {
            rows +=
                i64::try_from(table_rows(dev.pool(), table).await.len()).expect("the count fits");
        }
    }
    assert_eq!(dev.pending_out().await, rows);
    let uploaded = dev.drain_outbox().await;
    assert_eq!(i64::try_from(uploaded.len()).expect("the count fits"), rows);

    acknowledge_everything(&dev).await;
    migrate_sqlite(dev.pool())
        .await
        .expect("a second start is quiet");
    assert_eq!(dev.pending_out().await, 0);
}

#[tokio::test]
async fn a_device_that_never_synced_queues_nothing_again() {
    let dev = Device::new().await;
    forget_the_requeue(&dev).await;

    migrate_sqlite(dev.pool()).await.expect("the catch-up runs");

    assert_eq!(dev.pending_out().await, 0);
}
