//! The money ledgers and shop rules on the cloud: web writes reach devices, device pushes load on
//! the web, snapshots carry them, and rows written before the cloud could send them are published
//! by the backfill.

use axum::http::StatusCode;
use serde_json::Value;
use simplebash_pos_backend::clients::indexes::ensure_indexes;
use simplebash_pos_backend::core::config::TenantMode;
use simplebash_pos_backend::core::tenancy::{Tenant, with_tenant};
use simplebash_pos_backend::modules::cash_categories::service::ensure_system_categories;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use simplebash_pos_backend::modules::sync::migration::migrate_mongo;
use std::collections::BTreeSet;

use crate::common;
use crate::common::{LEDGER_RESOURCES, create_ledger_records};
use crate::helpers::{
    SERIAL, admin_token, call, device_token, drop_db, push, record, reid, snapshot, start,
    statuses, wait_for,
};

/// A shop with its built-in cash categories and one record of every ledger kind, made on the web.
async fn shop_with_ledger(app: &common::TestApp, tid: &str) {
    with_tenant(
        Tenant::id(tid).expect("valid tenant id"),
        ensure_system_categories(&app.db_handle),
    )
    .await
    .expect("the built-in categories are seeded");
    create_ledger_records(&app.router, &admin_token(app, tid)).await;
}

fn resources_of(changes: &[Value]) -> BTreeSet<String> {
    changes
        .iter()
        .filter_map(|change| change["resource"].as_str().map(str::to_owned))
        .collect()
}

fn missing_from(changes: &[Value]) -> Vec<&'static str> {
    let present = resources_of(changes);
    LEDGER_RESOURCES
        .iter()
        .map(|(resource, _)| *resource)
        .filter(|resource| !present.contains(*resource))
        .collect()
}

#[tokio::test]
async fn ledger_records_travel_from_the_web_to_a_device_and_back() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;
    shop_with_ledger(&app, "tnt_ledger").await;

    // Web writes reach a device through the change feed.
    let reader = device_token(&app, "tnt_ledger", "dev_ledger").await;
    wait_for(&app, &reader, 0, |changes| missing_from(changes).is_empty()).await;

    // A snapshot (how a new device starts) carries them too.
    let changes = snapshot(&app, &reader).await;
    assert_eq!(missing_from(&changes), Vec::<&str>::new());

    // The same records pushed by another shop's device load on that shop's web screens.
    let pushed: Vec<Value> = changes
        .iter()
        .filter(|change| {
            LEDGER_RESOURCES
                .iter()
                .any(|(resource, _)| change["resource"] == *resource)
        })
        .map(|change| {
            let resource = change["resource"].as_str().expect("a resource name");
            record(resource, &reid(&change["payload"]), "dev_other")
        })
        .collect();
    let (status, body) = push(&app, "tnt_ledger_two", "dev_other", "ledger-1", pushed).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        statuses(&body).iter().all(|status| status == "applied"),
        "{body}"
    );
    let web = admin_token(&app, "tnt_ledger_two");
    for uri in [
        "/api/cash-categories",
        "/api/payees",
        "/api/recurring-expenses",
        "/api/incomes",
        "/api/accounts",
        "/api/balances/staff",
        "/api/cash-settings",
        "/api/repair-services",
    ] {
        let (status, body) = call(&app.router, "GET", uri, Some(&web), None).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
    drop_db(&app).await;
}

#[tokio::test]
async fn the_backfill_publishes_rows_the_feed_never_had() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    // A repeat is refused by the unique index on `source_token`, which the server creates when it
    // starts and the tool relies on.
    ensure_indexes(app.mongo(), TenantMode::Multi).await;
    let tid = "tnt_backfill";
    // Written before anything watches the database, so no change is captured for them.
    shop_with_ledger(&app, tid).await;
    start(&app).await;
    let reader = device_token(&app, tid, "dev_backfill").await;
    let feed = || async {
        let (status, body) = call(
            &app.router,
            "GET",
            "/api/sync/pull?since=0&limit=500",
            Some(&reader),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"]["changes"]
            .as_array()
            .expect("changes is an array")
            .clone()
    };
    assert_eq!(missing_from(&feed().await).len(), LEDGER_RESOURCES.len());

    let planned = migrate_mongo(&app.db_handle, Mode::DryRun)
        .await
        .expect("the dry run reads");
    assert!(planned.shops.contains_key(tid), "{planned:?}");
    assert_eq!(missing_from(&feed().await).len(), LEDGER_RESOURCES.len());

    migrate_mongo(&app.db_handle, Mode::Apply)
        .await
        .expect("the backfill publishes");
    let published = feed().await;
    // Ready-made services always could be sent, so the backfill leaves them to the feed.
    assert_eq!(
        missing_from(&published),
        vec!["repairServices", "printServices"]
    );

    migrate_mongo(&app.db_handle, Mode::Apply)
        .await
        .expect("a repeat runs");
    assert_eq!(feed().await.len(), published.len(), "a repeat adds nothing");
    drop_db(&app).await;
}
