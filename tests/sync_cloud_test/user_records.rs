//! Accounts made on the website: devices receive them like every other synced record, and the ones
//! stored before they carried a version are numbered by the backfill.

use axum::http::StatusCode;
use mongodb::bson::{Document, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use simplebash_pos_backend::modules::sync::migration::migrate_user_versions;
use simplebash_pos_backend::modules::tenants::service::{ProvisionOwner, provision_shop};
use simplebash_pos_backend::modules::users::service::hash_password;
use std::collections::BTreeMap;

use crate::common;
use crate::helpers::{SERIAL, admin_token, call, device_token, drop_db, snapshot, start, wait_for};

const OWNER_PASSWORD: &str = "owner-pass-123";

/// A shop signed up on the website, with its owner and one more user added there.
async fn shop_with_website_users(app: &common::TestApp, tid: &str, shop_code: &str) {
    let owner = ProvisionOwner {
        username: "admin".to_string(),
        name: "Web Owner".to_string(),
        password_hash: hash_password(OWNER_PASSWORD).expect("the password hashes"),
    };
    provision_shop(&app.db_handle, tid, shop_code, "Web Users", Some(owner))
        .await
        .expect("the shop is provisioned");
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/users",
        Some(&admin_token(app, tid)),
        Some(json!({
            "name": "Web Staff", "username": "webstaff",
            "password": "staff-pass-123", "role": "staff"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "staff: {body}");
}

/// Username and version of every user in `changes`.
fn users_in(changes: &[Value]) -> BTreeMap<String, i64> {
    changes
        .iter()
        .filter(|change| change["resource"] == "users")
        .map(|change| {
            let username = change["payload"]["username"].as_str().expect("a username");
            let version = change["version"].as_i64().expect("a version");
            (username.to_string(), version)
        })
        .collect()
}

/// Like `users_in`, leaving out deletions.
fn users_in_upserts(changes: &[Value]) -> BTreeMap<String, i64> {
    let upserts: Vec<Value> = changes
        .iter()
        .filter(|change| change["op"] == "upsert")
        .cloned()
        .collect();
    users_in(&upserts)
}

fn both_users_at_version_one() -> BTreeMap<String, i64> {
    BTreeMap::from([("admin".to_string(), 1), ("webstaff".to_string(), 1)])
}

#[tokio::test]
async fn website_users_reach_devices_by_download_and_change_feed() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;
    let tid = "tnt_web_users";
    shop_with_website_users(&app, tid, "web-users").await;
    let reader = device_token(&app, tid, "dev_web_users").await;

    assert_eq!(
        users_in(&snapshot(&app, &reader).await),
        both_users_at_version_one()
    );
    let feed = wait_for(&app, &reader, 0, |changes| users_in(changes).len() == 2).await;
    let changes = feed["changes"].as_array().expect("changes is an array");
    assert_eq!(users_in(changes), both_users_at_version_one());
    drop_db(&app).await;
}

async fn unnumbered_users(app: &common::TestApp) -> u64 {
    app.mongo()
        .collection::<Document>("users")
        .count_documents(doc! { "version": { "$exists": false } })
        .await
        .expect("the users are counted")
}

#[tokio::test]
async fn the_backfill_numbers_accounts_stored_without_a_version() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    let tid = "tnt_old_users";
    // Stored as they were before accounts had a version, one of them since deleted, and written
    // before anything watches the database, so no change is captured for them.
    shop_with_website_users(&app, tid, "old-users").await;
    let users = app.mongo().collection::<Document>("users");
    users
        .update_one(
            doc! { "username": "webstaff" },
            doc! { "$set": { "deleted_at": mongodb::bson::DateTime::now() } },
        )
        .await
        .expect("the account is deleted");
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/users",
        Some(&admin_token(&app, tid)),
        Some(json!({
            "name": "Second Staff", "username": "second",
            "password": "staff-pass-123", "role": "staff"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "staff: {body}");
    users
        .update_many(doc! {}, doc! { "$unset": { "version": "" } })
        .await
        .expect("the versions are removed");
    start(&app).await;
    let reader = device_token(&app, tid, "dev_old_users").await;

    // Until the backfill runs the shop cannot be downloaded, but its people still sign in.
    let (status, body) = call(
        &app.router,
        "GET",
        "/api/sync/snapshot",
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({ "shopCode": "old-users", "username": "admin", "password": OWNER_PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "sign-in: {body}");

    let planned = migrate_user_versions(&app.db_handle, Mode::DryRun)
        .await
        .expect("the dry run reads");
    assert_eq!(planned.shops, BTreeMap::from([(tid.to_string(), 3)]));
    assert_eq!(unnumbered_users(&app).await, 3, "a dry run writes nothing");

    let applied = migrate_user_versions(&app.db_handle, Mode::Apply)
        .await
        .expect("the backfill writes");
    assert_eq!(applied.shops, planned.shops);
    assert_eq!(unnumbered_users(&app).await, 0);

    let live = BTreeMap::from([("admin".to_string(), 1), ("second".to_string(), 1)]);
    assert_eq!(users_in(&snapshot(&app, &reader).await), live);
    // A device that was already linked receives them without downloading again.
    wait_for(&app, &reader, 0, |changes| {
        let users = users_in_upserts(changes);
        users == live
    })
    .await;

    let repeat = migrate_user_versions(&app.db_handle, Mode::Apply)
        .await
        .expect("a repeat runs");
    assert!(repeat.shops.is_empty(), "{repeat:?}");
    drop_db(&app).await;
}
