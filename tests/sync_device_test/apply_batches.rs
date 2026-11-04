//! Apply batches tests.

use axum::http::StatusCode;

use serde_json::{Value, json};

use simplebash_pos_backend::domain::sequences::ReserveSequenceRequest;
use simplebash_pos_backend::domain::sync_v2::DerivedScope;

use simplebash_pos_backend::modules::sync::derived_sqlite;

use crate::helpers::{
    Device, SYNCED, as_pulled, assert_same_data, create_cashier, login_status, shop, table_rows,
};

// ---------------------------------------------------------------------------
// Joining an existing shop: the downloaded admin replaces the first-run setup
// ---------------------------------------------------------------------------

async fn setup_status(dev: &Device) -> Value {
    dev.call("GET", "/api/system/setup-status", None, None)
        .await
        .1["data"]
        .clone()
}

/// The cloud's view of a shop that was already set up: its admin plus a shop's data.
/// Returns the pulled changes, users last (as the cloud snapshot orders them).
async fn cloud_shop_with_admin() -> Vec<Value> {
    let cloud = Device::new().await;
    // The owner's POS admin exists before the device is linked, like the one
    // provisioned from web sign-up; enabling captures every existing row.
    let (status, res) = cloud
        .call(
            "POST",
            "/api/system/setup",
            Some(json!({
                "loadSampleData": false,
                "adminName": "Shop Owner",
                "adminPassword": "owner-pos-pass-1"
            })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    cloud
        .sync("POST", "/api/sync/enable", Some(json!({})))
        .await;
    shop(&cloud).await;
    as_pulled(&cloud.drain_outbox().await)
}

fn split_users_last(changes: &[Value]) -> (Vec<Value>, Vec<Value>) {
    changes
        .iter()
        .cloned()
        .partition(|c| c["resource"] != "users")
}

#[tokio::test]
async fn applying_the_same_batch_twice_changes_nothing() {
    let a = Device::new().await;
    let b = Device::new().await;
    a.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    shop(&a).await;
    let changes = as_pulled(&a.drain_outbox().await);

    let body = json!({ "mode": "incremental", "changes": changes });
    let first = b.sync("POST", "/api/sync/apply", Some(body.clone())).await;
    let before: Vec<Vec<Value>> = {
        let mut v = Vec::new();
        for t in SYNCED {
            v.push(table_rows(b.pool(), t).await);
        }
        v
    };
    let second = b.sync("POST", "/api/sync/apply", Some(body)).await;
    assert_eq!(second["applied"], 0, "nothing new: {second}");
    assert_eq!(
        second["duplicates"], first["applied"],
        "every change is recognised as already applied"
    );
    for (i, t) in SYNCED.iter().enumerate() {
        assert_eq!(
            before[i],
            table_rows(b.pool(), t).await,
            "table {t} changed on re-apply"
        );
    }
}

#[tokio::test]
async fn derived_columns_are_never_taken_from_a_payload() {
    let a = Device::new().await;
    let b = Device::new().await;
    a.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    shop(&a).await;
    let mut changes = as_pulled(&a.drain_outbox().await);

    for change in changes.iter_mut() {
        match change["resource"].as_str().expect("the value is a string") {
            "products" => change["payload"]["stockQuantity"] = json!(999),
            "customers" => {
                change["payload"]["totalPurchasesCents"] = json!(123_456);
                change["payload"]["outstandingBalanceCents"] = json!(654_321);
            }
            "invoices" => {
                change["payload"]["refundedCents"] = json!(777);
                change["payload"]["creditNoteCount"] = json!(9);
            }
            _ => {}
        }
    }
    b.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "incremental", "changes": changes })),
    )
    .await;
    assert_same_data(&a, &b).await;
}

#[tokio::test]
async fn recompute_restores_customer_balances_and_invoice_totals_from_the_ledger() {
    let dev = Device::new().await;
    let keys = shop(&dev).await;

    let purchases: i64 = sqlx::query_scalar("SELECT total_purchases_cents FROM customers")
        .fetch_one(dev.pool())
        .await
        .expect("the awaited step succeeds");
    let refunded: i64 = sqlx::query_scalar("SELECT refunded_cents FROM invoices")
        .fetch_one(dev.pool())
        .await
        .expect("the awaited step succeeds");

    sqlx::query("UPDATE customers SET total_purchases_cents = 1, outstanding_balance_cents = 2")
        .execute(dev.pool())
        .await
        .expect("the awaited step succeeds");
    sqlx::query("UPDATE invoices SET refunded_cents = 0, credit_note_count = 0")
        .execute(dev.pool())
        .await
        .expect("the awaited step succeeds");
    sqlx::query("UPDATE products SET stock_quantity = 0")
        .execute(dev.pool())
        .await
        .expect("the awaited step succeeds");

    let mut scope = DerivedScope::default();
    scope.customer_keys.insert(
        keys["customerKey"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
    );
    scope.invoice_keys.insert(
        keys["invoiceKey"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
    );
    scope.product_ids.insert(
        keys["productId"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
    );
    let mut conn = dev
        .pool()
        .acquire()
        .await
        .expect("the awaited step succeeds");
    let conflicts = derived_sqlite::recompute(&mut conn, &scope)
        .await
        .expect("the awaited step succeeds");
    assert!(conflicts.is_empty(), "{conflicts:?}");
    drop(conn);

    let again: i64 = sqlx::query_scalar("SELECT total_purchases_cents FROM customers")
        .fetch_one(dev.pool())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(again, purchases);
    let outstanding: i64 = sqlx::query_scalar("SELECT outstanding_balance_cents FROM customers")
        .fetch_one(dev.pool())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(outstanding, 0, "a fully paid cash sale leaves no balance");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT refunded_cents FROM invoices")
            .fetch_one(dev.pool())
            .await
            .expect("the awaited step succeeds"),
        refunded
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT credit_note_count FROM invoices")
            .fetch_one(dev.pool())
            .await
            .expect("the awaited step succeeds"),
        1
    );
    let stock: i64 = sqlx::query_scalar("SELECT stock_quantity FROM products")
        .fetch_one(dev.pool())
        .await
        .expect("the awaited step succeeds");
    let ledger: i64 = sqlx::query_scalar("SELECT SUM(quantity_delta) FROM stock_movements")
        .fetch_one(dev.pool())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(stock, ledger);
}

#[tokio::test]
async fn a_newer_edit_wins_and_the_losing_edit_is_kept_for_review() {
    let a = Device::new().await;
    let b = Device::new().await;
    a.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    b.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    let keys = shop(&a).await;
    b.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "incremental", "changes": as_pulled(&a.drain_outbox().await) })),
    )
    .await;
    a.sync(
        "POST",
        "/api/sync/outbox/ack",
        Some(json!({ "upToSeq": 1_000_000 })),
    )
    .await;
    b.sync(
        "POST",
        "/api/sync/outbox/ack",
        Some(json!({ "upToSeq": 1_000_000 })),
    )
    .await;

    // Both devices rename the same product; B's edit is later.
    let id = keys["productId"].as_str().expect("the value is a string");
    a.api(
        "PUT",
        &format!("/api/inventory/products/{id}"),
        Some(json!({ "name": "Phone (A)" })),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    b.api(
        "PUT",
        &format!("/api/inventory/products/{id}"),
        Some(json!({ "name": "Phone (B)" })),
    )
    .await;

    let from_a = as_pulled(&a.drain_outbox().await);
    let from_b = as_pulled(&b.drain_outbox().await);

    // A receives B's newer edit: B wins on both replicas regardless of order.
    a.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "incremental", "changes": from_b })),
    )
    .await;
    let applied = b
        .sync(
            "POST",
            "/api/sync/apply",
            Some(json!({ "mode": "incremental", "changes": from_a })),
        )
        .await;
    assert_eq!(applied["applied"], 0, "A's older edit lost on B: {applied}");
    for dev in [&a, &b] {
        let name: String = sqlx::query_scalar("SELECT name FROM products")
            .fetch_one(dev.pool())
            .await
            .expect("the awaited step succeeds");
        assert_eq!(name, "Phone (B)");
    }

    // The loser is recorded, listed, and can be resolved.
    let conflicts = b.sync("GET", "/api/sync/conflicts", None).await;
    let conflict = conflicts
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|c| c["kind"] == "LWW_LOSER")
        .expect("LWW loser recorded");
    assert_eq!(conflict["detail"]["losingPayload"]["name"], "Phone (A)");
    assert_eq!(
        b.state().await["conflictsOpen"]
            .as_i64()
            .expect("the value is an integer"),
        1
    );
    let key = conflict["key"].as_str().expect("the value is a string");
    b.sync(
        "POST",
        &format!("/api/sync/conflicts/{key}/resolve"),
        Some(json!({ "resolution": "kept newer edit" })),
    )
    .await;
    assert_eq!(
        b.state().await["conflictsOpen"]
            .as_i64()
            .expect("the value is an integer"),
        0
    );
}

#[tokio::test]
async fn bootstrap_replaces_local_data_and_only_the_first_page_wipes() {
    let a = Device::new().await;
    a.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    shop(&a).await;
    let changes = as_pulled(&a.drain_outbox().await);
    let (first_page, second_page) = changes.split_at(changes.len() / 2);

    let b = Device::new().await;
    b.api(
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "Local demo data", "icon": "x", "color": "red" })),
    )
    .await;

    b.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "bootstrap", "changes": first_page })),
    )
    .await;
    assert_eq!(b.state().await["bootstrapActive"], true);
    let demo: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM categories WHERE name = 'Local demo data'")
            .fetch_one(b.pool())
            .await
            .expect("the awaited step succeeds");
    assert_eq!(demo, 0, "the first bootstrap page wiped the previous data");

    b.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "bootstrap", "changes": second_page, "advanceCursorTo": 7 })),
    )
    .await;
    let state = b.state().await;
    assert_eq!(state["bootstrapActive"], false);
    assert_eq!(state["cloudCursor"], 7);
    assert_same_data(&a, &b).await;
}

#[tokio::test]
async fn downloading_a_shop_with_an_admin_completes_setup_and_the_admin_can_log_in() {
    let changes = cloud_shop_with_admin().await;
    assert!(changes.iter().any(|c| c["resource"] == "users"));

    let joining = Device::new().await;
    let before = setup_status(&joining).await;
    assert_eq!(before["setupCompleted"], false);
    assert_eq!(before["isFirstRun"], true);

    joining
        .sync(
            "POST",
            "/api/sync/apply",
            Some(json!({ "mode": "bootstrap", "changes": changes, "advanceCursorTo": 9, "setupCompleted": true })),
        )
        .await;

    let after = setup_status(&joining).await;
    assert_eq!(after["setupCompleted"], true);
    assert_eq!(after["isFirstRun"], false);
    assert_eq!(after["sampleDataLoaded"], false);
    assert_eq!(
        login_status(&joining, "admin", "owner-pos-pass-1").await,
        StatusCode::OK
    );
    // And the one-time setup can no longer create a second admin.
    let (status, res) = joining
        .call(
            "POST",
            "/api/system/setup",
            Some(json!({ "loadSampleData": false, "adminPassword": "another-pass-1" })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
}

#[tokio::test]
async fn a_half_downloaded_shop_is_not_marked_as_set_up() {
    let changes = cloud_shop_with_admin().await;
    let (rest, users) = split_users_last(&changes);

    let joining = Device::new().await;
    joining
        .sync(
            "POST",
            "/api/sync/apply",
            Some(json!({ "mode": "bootstrap", "changes": rest })),
        )
        .await;
    assert_eq!(setup_status(&joining).await["setupCompleted"], false);

    joining
        .sync(
            "POST",
            "/api/sync/apply",
            Some(json!({ "mode": "bootstrap", "changes": users, "advanceCursorTo": 9, "setupCompleted": true })),
        )
        .await;
    assert_eq!(setup_status(&joining).await["setupCompleted"], true);
}

#[tokio::test]
async fn downloading_a_shop_without_an_admin_leaves_the_setup_to_the_owner() {
    let cloud = Device::new().await;
    cloud
        .sync("POST", "/api/sync/enable", Some(json!({})))
        .await;
    create_cashier(&cloud, "cash", "cashier-pass-1").await;
    let changes = as_pulled(&cloud.drain_outbox().await);

    let joining = Device::new().await;
    joining
        .sync(
            "POST",
            "/api/sync/apply",
            Some(json!({ "mode": "bootstrap", "changes": changes, "advanceCursorTo": 3, "setupCompleted": true })),
        )
        .await;

    let status = setup_status(&joining).await;
    assert_eq!(status["setupCompleted"], false);
}

#[tokio::test]
async fn downloading_the_same_shop_again_keeps_the_original_setup_time() {
    let changes = cloud_shop_with_admin().await;
    let joining = Device::new().await;
    let body = json!({ "mode": "bootstrap", "changes": changes, "advanceCursorTo": 9, "setupCompleted": true });
    joining
        .sync("POST", "/api/sync/apply", Some(body.clone()))
        .await;
    let first = setup_status(&joining).await["setupCompletedAt"].clone();
    assert!(first.is_string());

    joining.sync("POST", "/api/sync/apply", Some(body)).await;
    let again = setup_status(&joining).await;
    assert_eq!(again["setupCompleted"], true);
    assert_eq!(again["setupCompletedAt"], first);
}

#[tokio::test]
async fn a_shop_the_cloud_has_not_set_up_leaves_the_choice_to_the_owner_with_the_cloud_admin() {
    let changes = cloud_shop_with_admin().await;

    for flag in [json!(null), json!(false)] {
        let joining = Device::new().await;
        let mut body = json!({ "mode": "bootstrap", "changes": changes, "advanceCursorTo": 9 });
        if !flag.is_null() {
            body["setupCompleted"] = flag;
        }
        joining.sync("POST", "/api/sync/apply", Some(body)).await;

        // The admin arrived, but the setup choice (demo vs clean) is still open.
        let status = setup_status(&joining).await;
        assert_eq!(status["setupCompleted"], false);
        assert_eq!(
            status["isFirstRun"], false,
            "users exist, so this is not a blank install"
        );

        // A wrong password changes nothing.
        let (code, res) = joining
            .call(
                "POST",
                "/api/system/setup",
                Some(json!({
                    "loadSampleData": false,
                    "adminPassword": "not-the-password"
                })),
                None,
            )
            .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{res}");
        assert_eq!(setup_status(&joining).await["setupCompleted"], false);

        // The cloud admin's own password completes it, signs in, and adds no second admin.
        let (code, res) = joining
            .call(
                "POST",
                "/api/system/setup",
                Some(json!({
                    "loadSampleData": false,
                    "adminPassword": "owner-pos-pass-1"
                })),
                None,
            )
            .await;
        assert_eq!(code, StatusCode::OK, "{res}");
        assert!(res["data"]["token"].is_string(), "{res}");
        let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
            .fetch_one(joining.pool())
            .await
            .expect("the awaited step succeeds");
        assert_eq!(admins, 1);
        assert_eq!(setup_status(&joining).await["setupCompleted"], true);
    }
}

#[tokio::test]
async fn a_set_up_cloud_shop_passes_on_whether_it_loaded_demo_data() {
    let changes = cloud_shop_with_admin().await;
    let joining = Device::new().await;
    joining
        .sync(
            "POST",
            "/api/sync/apply",
            Some(json!({
                "mode": "bootstrap", "changes": changes, "advanceCursorTo": 9,
                "setupCompleted": true, "sampleDataLoaded": true
            })),
        )
        .await;
    let status = setup_status(&joining).await;
    assert_eq!(status["setupCompleted"], true);
    assert_eq!(status["sampleDataLoaded"], true);
}

// ---------------------------------------------------------------------------
// Number blocks and backup
// ---------------------------------------------------------------------------

#[tokio::test]
async fn linked_devices_draw_numbers_from_blocks_then_a_unique_fallback() {
    use simplebash_pos_backend::modules::sequences::service::reserve_sequence;
    let dev = Device::new().await;
    let single = || ReserveSequenceRequest {
        block_size: Some(1),
        device_id: None,
    };

    // Unlinked: the ordinary local counter.
    let plain = reserve_sequence(&dev.app.db_handle, "invoice".to_string(), single())
        .await
        .expect("the awaited step succeeds");
    assert_eq!((plain.prefix.as_str(), plain.start), ("INV-", 1));

    dev.sync(
        "POST",
        "/api/sync/enable",
        Some(json!({ "deviceId": "dev_zzzz0000aaaa1111" })),
    )
    .await;
    assert_eq!(dev.state().await["deviceId"], "dev_zzzz0000aaaa1111");
    dev.sync(
        "POST",
        "/api/sync/blocks",
        Some(json!({ "name": "invoice", "prefix": "INV-", "padding": 6, "start": 500, "end": 501, "expiresAt": "2099-01-01T00:00:00Z" })),
    )
    .await;
    let blocks = dev.state().await["numberBlocks"].clone();
    assert_eq!(blocks[0]["name"], "invoice");
    assert_eq!(blocks[0]["remaining"], 2);
    assert_eq!(blocks[0]["blockSize"], 2);

    let n1 = reserve_sequence(&dev.app.db_handle, "invoice".to_string(), single())
        .await
        .expect("the awaited step succeeds");
    let n2 = reserve_sequence(&dev.app.db_handle, "invoices".to_string(), single())
        .await
        .expect("the awaited step succeeds");
    assert_eq!((n1.prefix.as_str(), n1.start), ("INV-", 500));
    assert_eq!((n2.prefix.as_str(), n2.start), ("INV-", 501));
    assert_eq!(dev.state().await["numberBlocks"][0]["remaining"], 0);

    // Block exhausted: per-device fallback series, never colliding with cloud numbers.
    let f1 = reserve_sequence(&dev.app.db_handle, "invoice".to_string(), single())
        .await
        .expect("the awaited step succeeds");
    let f2 = reserve_sequence(&dev.app.db_handle, "invoice".to_string(), single())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(f1.prefix, "INV-Dzzzz-");
    assert_eq!((f1.start, f2.start), (1, 2));
    let rendered = |r: &simplebash_pos_backend::domain::sequences::SequenceReservationResponse| {
        format!("{}{:0w$}", r.prefix, r.start, w = r.padding)
    };
    let all = [rendered(&n1), rendered(&n2), rendered(&f1), rendered(&f2)];
    let unique: std::collections::HashSet<_> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "{all:?}");
}

/// SKU and barcode draw from the same per-prefix/per-namespace counters a
/// direct (unlinked, or block-less) generation uses (see
/// `inventory::service::sku::generate_sku` and
/// `barcode::service::generator::generate`), so a linked device consuming a
/// pre-fetched block can never mint the same SKU/barcode as one generated
/// directly - and once the block runs dry, generation keeps going without a
/// gap or a collision instead of erroring.
#[tokio::test]
async fn linked_devices_draw_skus_and_barcodes_from_blocks_then_a_unique_fallback() {
    use simplebash_pos_backend::modules::sequences::service::reserve_sequence;

    let dev = Device::new().await;
    dev.sync(
        "POST",
        "/api/sync/enable",
        Some(json!({ "deviceId": "dev_zzzz0000aaaa1111" })),
    )
    .await;

    let cat = dev
        .api(
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": "Phone Repairs", "icon": "x", "color": "red" })),
        )
        .await;
    let category_key = cat["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let sub = dev
        .api(
            "POST",
            &format!("/api/inventory/categories/{category_key}/subcategories"),
            Some(json!({ "name": "Screens" })),
        )
        .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let new_product = |name: &str| {
        json!({
            "name": name, "categoryKey": &category_key, "subcategoryKey": &subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "minStockThreshold": 1,
            "autoGenerateBarcode": true
        })
    };

    // --- SKU: PHO-SCR is derived from "Phone Repairs" + "Screens" ----------
    let sku_block = reserve_sequence(
        &dev.app.db_handle,
        "sku:PHO-SCR".to_string(),
        ReserveSequenceRequest {
            block_size: Some(2),
            device_id: None,
        },
    )
    .await
    .expect("the awaited step succeeds");
    assert_eq!(
        (
            sku_block.prefix.as_str(),
            sku_block.padding,
            sku_block.start,
            sku_block.end
        ),
        ("PHO-SCR-", 4, 1, 2)
    );
    dev.sync(
        "POST",
        "/api/sync/blocks",
        Some(json!({ "name": "sku:PHO-SCR", "prefix": "PHO-SCR-", "padding": 4, "start": 1, "end": 2, "expiresAt": "2099-01-01T00:00:00Z" })),
    )
    .await;

    // --- barcode: one global "product" namespace ----------------------------
    let barcode_block = reserve_sequence(
        &dev.app.db_handle,
        "barcode".to_string(),
        ReserveSequenceRequest {
            block_size: Some(2),
            device_id: None,
        },
    )
    .await
    .expect("the awaited step succeeds");
    assert_eq!((barcode_block.start, barcode_block.end), (1, 2));
    dev.sync(
        "POST",
        "/api/sync/blocks",
        Some(json!({ "name": "barcode", "prefix": "", "padding": 0, "start": 1, "end": 2, "expiresAt": "2099-01-01T00:00:00Z" })),
    )
    .await;

    let extract_barcode_seq =
        |barcode: &str| -> i64 { barcode[2..12].parse().expect("the text parses") };

    // Products 1 and 2 draw the block, in order, exactly once each.
    let p1 = dev
        .api(
            "POST",
            "/api/inventory/products",
            Some(new_product("Screen A")),
        )
        .await;
    assert_eq!(p1["data"]["sku"], "PHO-SCR-0001");
    assert_eq!(
        extract_barcode_seq(
            p1["data"]["barcode"]
                .as_str()
                .expect("the value is a string")
        ),
        1
    );

    let p2 = dev
        .api(
            "POST",
            "/api/inventory/products",
            Some(new_product("Screen B")),
        )
        .await;
    assert_eq!(p2["data"]["sku"], "PHO-SCR-0002");
    assert_eq!(
        extract_barcode_seq(
            p2["data"]["barcode"]
                .as_str()
                .expect("the value is a string")
        ),
        2
    );

    // Both blocks are exhausted now - a third product must still get fresh,
    // never-seen-before numbers instead of erroring or repeating 1/2.
    let p3 = dev
        .api(
            "POST",
            "/api/inventory/products",
            Some(new_product("Screen C")),
        )
        .await;
    let sku3 = p3["data"]["sku"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let barcode3_seq = extract_barcode_seq(
        p3["data"]["barcode"]
            .as_str()
            .expect("the value is a string"),
    );

    // SKU's exhausted-block fallback is the same device-tagged text series
    // invoice/etc. already use (see the test above) - a device-local counter,
    // impossible to collide with the block's PHO-SCR-0001/0002.
    assert_eq!(sku3, "PHO-SCR-Dzzzz-0001");
    // Barcode has no room for text in an EAN-13 payload, so its fallback tags
    // the device numerically instead (see `generator::generate`): the top
    // digits of the 10-digit sequence field are the device's tag, so a
    // fallback sequence is always >= 10,000,000 - far outside the 1/2 the
    // block itself ever handed out, so it can't collide with them.
    assert!(
        barcode3_seq >= 10_000_000,
        "expected a device-tagged fallback sequence, got {barcode3_seq}"
    );

    // Everything actually minted this run is distinct.
    let skus = [
        p1["data"]["sku"].as_str().expect("the value is a string"),
        p2["data"]["sku"].as_str().expect("the value is a string"),
        sku3.as_str(),
    ];
    let unique_skus: std::collections::HashSet<_> = skus.iter().collect();
    assert_eq!(unique_skus.len(), skus.len(), "{skus:?}");
    let barcodes = [1i64, 2, barcode3_seq];
    let unique_barcodes: std::collections::HashSet<_> = barcodes.iter().collect();
    assert_eq!(unique_barcodes.len(), barcodes.len(), "{barcodes:?}");
}
