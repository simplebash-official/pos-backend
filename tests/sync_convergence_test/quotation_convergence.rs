//! Quotation convergence tests.

use mongodb::bson::{Document, doc};
use serde_json::json;

use sqlx::SqlitePool;
use std::collections::BTreeSet;

use crate::common;
use crate::helpers::{
    Device, TENANT, bootstrap, cloud_changes, cloud_conflicts, keys, pull_all, push_all,
    reserve_blocks, start_consumer, table_rows, wait_for_cloud_quiet,
};

/// Two devices work on quotations offline: both cancel the same one and each makes a new
/// one of its own. Cancelling is the only change a quotation ever gets, so whichever order
/// it reaches the cloud in, everybody must end up with the same three quotations, the
/// shared one cancelled, three different numbers, and no change rejected or renamed.
#[tokio::test]
async fn concurrent_quotation_cancels_and_creates_converge_across_devices_and_the_cloud() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;

    let mut devices = vec![Device::new(41, &cloud).await, Device::new(42, &cloud).await];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
        reserve_blocks(&cloud, d).await;
    }

    let new_quotation = |who: &str| {
        json!({
            "customer": { "customerName": who },
            "items": [{ "name": "Screen replacement", "unitPriceCents": 10000, "quantity": 1, "discountCents": 0 }],
            "validUntil": "2099-12-31"
        })
    };

    // Device 1 makes a quotation and uploads it; device 2 joins and receives it.
    let shared = devices[0]
        .api("POST", "/api/quotations", Some(new_quotation("Shared")))
        .await["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    bootstrap(&cloud, &mut devices[1]).await;

    // Offline: both cancel the shared one, and each makes its own.
    let mut own = Vec::new();
    for d in &devices {
        d.api("POST", &format!("/api/quotations/{shared}/cancel"), None)
            .await;
        let made = d
            .api("POST", "/api/quotations", Some(new_quotation(&d.id)))
            .await;
        own.push(
            made["data"]["key"]
                .as_str()
                .expect("the value is a string")
                .to_string(),
        );
    }

    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await; // fails the test if the cloud rejects anything
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in devices.iter_mut() {
            pull_all(&cloud, d).await;
        }
    }

    for d in &devices {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT key, status FROM quotations ORDER BY key")
                .fetch_all(d.pool())
                .await
                .expect("the awaited step succeeds");
        assert_eq!(rows.len(), 3, "three quotations on {}: {rows:?}", d.id);
        let status_of = |key: &str| {
            rows.iter()
                .find(|(k, _)| k == key)
                .unwrap_or_else(|| panic!("{key} missing on {}", d.id))
                .1
                .clone()
        };
        assert_eq!(status_of(&shared), "cancelled", "on {}", d.id);
        assert_eq!(status_of(&own[0]), "active", "on {}", d.id);
        assert_eq!(status_of(&own[1]), "active", "on {}", d.id);

        let numbers: BTreeSet<String> =
            sqlx::query_scalar::<_, String>("SELECT quotation_number FROM quotations")
                .fetch_all(d.pool())
                .await
                .expect("the awaited step succeeds")
                .into_iter()
                .collect();
        assert_eq!(numbers.len(), 3, "numbers must not collide: {numbers:?}");
        assert!(numbers.iter().all(|n| n.starts_with("QUO-")), "{numbers:?}");

        let renamed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sync_conflicts WHERE kind = 'UNIQUE_VIOLATION'",
        )
        .fetch_one(d.pool())
        .await
        .expect("the awaited step succeeds");
        assert_eq!(renamed, 0, "{} had to renumber a quotation", d.id);
    }
    assert_eq!(
        table_rows(devices[0].pool(), "quotations").await,
        table_rows(devices[1].pool(), "quotations").await,
        "the two devices must hold identical quotations"
    );

    // The cloud holds the same three, with the shared one cancelled (a version past its creation).
    let stored = cloud
        .mongo()
        .collection::<Document>("quotations")
        .find_one(doc! { "tenant_id": TENANT, "key": &shared })
        .await
        .expect("the awaited step succeeds")
        .expect("the cloud holds the shared quotation");
    assert_eq!(
        stored
            .get_str("status")
            .expect("the document has this string field"),
        "cancelled"
    );
    assert!(
        stored
            .get_i64("version")
            .expect("the document has this field")
            >= 2
    );
    let in_cloud: BTreeSet<String> = cloud_changes(&cloud)
        .await
        .iter()
        .filter(|c| c["resource"] == "quotations" && c["op"] == "upsert")
        .map(|c| {
            c["key"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        })
        .collect();
    assert_eq!(in_cloud, keys(devices[0].pool(), "quotations").await);

    cloud.mongo().drop().await.ok();
}

/// Two devices independently create a category with the SAME name, offline.
/// Business rules cannot stop this (each device only ever sees its own local
/// row), so the merge is the last line of defense: the losing name is renamed
/// with a device-tag suffix and reported, rather than one category silently
/// overwriting or shadowing the other.
#[tokio::test]
async fn concurrent_duplicate_category_names_are_renamed_and_both_survive() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;

    let mut devices = vec![Device::new(41, &cloud).await, Device::new(42, &cloud).await];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
    }
    // A shared baseline so both devices have something to bootstrap/converge on.
    devices[0]
        .api(
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": "Baseline", "icon": "box", "color": "gray" })),
        )
        .await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    bootstrap(&cloud, &mut devices[1]).await;

    // Offline: both devices create a category with the identical name.
    let mut category_keys = Vec::new();
    for d in &devices {
        let cat = d
            .api(
                "POST",
                "/api/inventory/categories",
                Some(json!({ "name": "Accessories", "icon": "tag", "color": "purple" })),
            )
            .await;
        category_keys.push(
            cat["data"]["key"]
                .as_str()
                .expect("the value is a string")
                .to_string(),
        );
    }
    assert_ne!(category_keys[0], category_keys[1]);

    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await;
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in devices.iter_mut() {
            pull_all(&cloud, d).await;
        }
    }

    // Both categories survive on every replica, and their names are now distinct.
    for d in &devices {
        let names: Vec<String> =
            sqlx::query_scalar("SELECT name FROM categories WHERE key IN (?, ?) ORDER BY key")
                .bind(&category_keys[0])
                .bind(&category_keys[1])
                .fetch_all(d.pool())
                .await
                .expect("the awaited step succeeds");
        assert_eq!(names.len(), 2, "both categories must survive on {}", d.id);
        assert_ne!(
            names[0], names[1],
            "the colliding name must have been renamed apart on {}",
            d.id
        );
    }
    // Fixed bug: which of the two keys kept the original name and which got the
    // device-tag suffix used to be decided independently by whichever replica
    // was CURRENTLY APPLYING when it hit the collision - device A would rename
    // B's incoming create while device B, applying A's incoming create against
    // its own already-stored row, renamed A's instead, so both devices ended up
    // with the same two names as a SET but assigned to SWAPPED keys. The fix
    // makes the winner deterministic and identical everywhere (the
    // lexicographically LOWER key always keeps the original value) and
    // republishes a locally-decided correction from inside the applying
    // transaction instead of letting it get stuck there. Assert the exact
    // per-key assignment now agrees everywhere, not just the set of names.
    let (lower_key, higher_key) = if category_keys[0] < category_keys[1] {
        (&category_keys[0], &category_keys[1])
    } else {
        (&category_keys[1], &category_keys[0])
    };

    async fn name_of(pool: &SqlitePool, key: &str) -> String {
        sqlx::query_scalar("SELECT name FROM categories WHERE key = ?")
            .bind(key)
            .fetch_one(pool)
            .await
            .expect("the awaited step succeeds")
    }
    async fn cloud_name_of(cloud: &common::TestApp, key: &str) -> String {
        cloud
            .mongo()
            .collection::<Document>("categories")
            .find_one(doc! { "tenant_id": TENANT, "key": key })
            .await
            .expect("the awaited step succeeds")
            .unwrap_or_else(|| panic!("{key} must exist on the cloud"))
            .get_str("name")
            .expect("the document has this string field")
            .to_string()
    }
    let cloud_lower = cloud_name_of(&cloud, lower_key).await;
    let cloud_higher = cloud_name_of(&cloud, higher_key).await;
    assert_eq!(
        cloud_lower, "Accessories",
        "the lower key must keep the original name"
    );
    assert_ne!(
        cloud_higher, "Accessories",
        "the higher key must have been renamed apart"
    );

    for d in &devices {
        assert_eq!(
            name_of(d.pool(), lower_key).await,
            cloud_lower,
            "{} disagrees with the cloud on the lower key's name",
            d.id
        );
        assert_eq!(
            name_of(d.pool(), higher_key).await,
            cloud_higher,
            "{} disagrees with the cloud on the higher key's name",
            d.id
        );
    }

    let cloud_hits = cloud_conflicts(&cloud, "UNIQUE_VIOLATION").await.len();
    let mut device_hits = 0i64;
    for d in &devices {
        device_hits += sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sync_conflicts WHERE kind = 'UNIQUE_VIOLATION'",
        )
        .fetch_one(d.pool())
        .await
        .expect("the awaited step succeeds");
    }
    assert!(
        cloud_hits + device_hits as usize >= 1,
        "no UNIQUE_VIOLATION conflict was recorded anywhere"
    );

    cloud.mongo().drop().await.ok();
}
