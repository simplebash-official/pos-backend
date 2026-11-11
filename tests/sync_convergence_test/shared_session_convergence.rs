//! End-to-end convergence tests for shared shop sessions, shop profiles,
//! drawer state, cash movements, offline dual-open conflicts, and mode toggles
//! across multiple offline SQLite devices and MongoDB Atlas.

use axum::http::StatusCode;
use jsonwebtoken::{EncodingKey, Header, encode};
use mongodb::bson::{Document, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::middleware::auth::Claims;
use simplebash_pos_backend::core::tenancy::{Tenant, with_tenant};
use simplebash_pos_backend::domain::cash_categories::category_key;
use simplebash_pos_backend::domain::sessions::CashMovementType;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::modules::settings::service::{SHOP_PROFILE_KEY, ensure_shop_profile};

use crate::common;
use crate::helpers::{
    Device, TENANT, bootstrap, call, cloud_conflicts, keys, pull_all, push_all, reserve_blocks,
    start_consumer, table_rows, wait_for_cloud_quiet,
};

const SHARED_SESSION_TABLES: &[&str] = &[
    "shop_profiles",
    "drawer_state",
    "register_sessions",
    "cash_movements",
    "categories",
    "subcategories",
    "products",
    "customers",
    "invoices",
    "payments",
    "credit_notes",
];

fn user_token(config: &Config, user_id: &str, role: Role, name: &str) -> String {
    let perms = roles::default_permissions(role);
    let claims = Claims {
        sub: user_id.to_string(),
        exp: 9_999_999_999,
        role: Some(role),
        permissions: perms.iter().map(|p| (*p).to_string()).collect(),
        tid: None,
        scope: None,
        did: None,
        name: Some(name.to_string()),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .expect("token encodes")
}

async fn assert_all_devices_match(devices: &[Device]) {
    let strip_singleton_origin = |table: &str, mut rows: Vec<Value>| {
        // Two offline devices can both create the `"shop"` drawer row before first
        // sync; `apply_records_in_tx` intentionally preserves local `id`/`created_at`.
        if table == "drawer_state" {
            for row in &mut rows {
                if let Some(obj) = row.as_object_mut() {
                    obj.remove("id");
                    obj.remove("_id");
                    obj.remove("created_at");
                }
            }
        }
        rows
    };
    for table in SHARED_SESSION_TABLES {
        let base = strip_singleton_origin(table, table_rows(devices[0].pool(), table).await);
        for d in &devices[1..] {
            assert_eq!(
                base,
                strip_singleton_origin(table, table_rows(d.pool(), table).await),
                "table {table} diverged between {} and {}",
                devices[0].id,
                d.id
            );
        }
    }
    for d in devices {
        let state = d.sync("GET", "/api/sync/state", None).await;
        assert_eq!(
            state["pendingOut"], 0,
            "{} has unacked outbox items: {state}",
            d.id
        );
    }
}

#[tokio::test]
async fn shared_shop_sessions_two_offline_devices_and_joining_bootstrap_converge() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;
    let _ = Tenant::id(TENANT).expect("valid tenant");

    let mut devices = vec![Device::new(31, &cloud).await, Device::new(32, &cloud).await];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
        reserve_blocks(&cloud, d).await;
    }

    // Step A: Device 1 initializes shop in SharedShop mode, seeds catalog, opens shift S1
    ensure_shop_profile(&devices[0].app.db_handle, Some("Conv Shared Shop"))
        .await
        .expect("shop profile initializes");
    devices[0]
        .api(
            "PUT",
            "/api/settings/shop-profile",
            Some(json!({
                "tradingName": "Conv Shared Shop",
                "addressLines": ["12 Main St", "Colombo"],
                "allowSharedUserSessions": true
            })),
        )
        .await;

    devices[0]
        .api(
            "POST",
            "/api/users",
            Some(json!({
                "name": "Manager PIN",
                "username": "mgr_pin",
                "password": "Password123!",
                "role": "manager",
            })),
        )
        .await;
    let mgr_login = devices[0]
        .api(
            "POST",
            "/api/auth/login",
            Some(json!({ "username": "mgr_pin", "password": "Password123!" })),
        )
        .await;
    let mgr_token = mgr_login["data"]["token"].as_str().expect("token");
    let (pin_status, pin_body) = call(
        &devices[0].app.router,
        "POST",
        "/api/auth/set-pin",
        mgr_token,
        Some(json!({ "pin": "1234" })),
    )
    .await;
    assert_eq!(pin_status, StatusCode::OK, "set-pin: {pin_body}");

    let cat = devices[0]
        .api(
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": "Accessories", "icon": "Box", "color": "blue" })),
        )
        .await;
    let cat_key = cat["data"]["key"].as_str().expect("key").to_string();
    let sub = devices[0]
        .api(
            "POST",
            &format!("/api/inventory/categories/{cat_key}/subcategories"),
            Some(json!({ "name": "Chargers" })),
        )
        .await;
    let sub_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("key")
        .to_string();
    let prod = devices[0]
        .api(
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": "Fast Charger",
                "categoryKey": cat_key,
                "subcategoryKey": sub_key,
                "warranty": { "coverage": { "kind": "none" } },
                "minStockThreshold": 2,
                "openingStock": { "quantity": 50, "unitCostCents": 300, "sellingPriceCents": 500 }
            })),
        )
        .await;
    let prod_key = prod["data"]["key"].as_str().expect("key").to_string();
    let cust = devices[0]
        .api(
            "POST",
            "/api/customers",
            Some(json!({ "name": "Nimal", "primaryPhone": "0775551234" })),
        )
        .await;
    let cust_key = cust["data"]["key"].as_str().expect("key").to_string();

    let s1 = devices[0]
        .api(
            "POST",
            "/api/sessions/open",
            Some(json!({ "terminalId": "term_1", "openingFloatCents": 10000 })),
        )
        .await;
    let s1_key = s1["data"]["key"].as_str().expect("key").to_string();

    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    pull_all(&cloud, &mut devices[1]).await;

    // Step B: Both work offline concurrently against the shared open shift S1
    devices[0]
        .api(
            "POST",
            "/api/billing/sales",
            Some(json!({
                "staff": { "cashierName": "D1" },
                "items": [{ "productKey": prod_key, "sellingPriceCents": 500, "quantity": 4, "discountCents": 0, "sourceType": "retail" }],
                "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 2000 },
                "shopProfileSnapshot": { "tradingName": "Conv Shared Shop" }
            })),
        )
        .await;
    devices[0]
        .api(
            "POST",
            "/api/billing/sales",
            Some(json!({
                "staff": { "cashierName": "D1" },
                "items": [{ "productKey": prod_key, "sellingPriceCents": 500, "quantity": 3, "discountCents": 0, "sourceType": "retail" }],
                "payment": { "paymentMethod": "card", "isCredit": false, "amountReceivedCents": 1500 },
                "shopProfileSnapshot": { "tradingName": "Conv Shared Shop" }
            })),
        )
        .await;
    let add1 = devices[0]
        .api(
            "POST",
            "/api/sessions/movements",
            Some(json!({
                "movementType": "add_cash",
                "amountCents": 1200,
                "sessionKey": s1_key,
                "terminalId": "term_1",
                "categoryKey": category_key(CashMovementType::AddCash, "Change float top-up")
            })),
        )
        .await;
    let add1_key = add1["data"]["key"].as_str().expect("key").to_string();
    devices[0]
        .api(
            "POST",
            &format!("/api/sessions/movements/{add1_key}/reverse"),
            Some(json!({ "reason": "Undo" })),
        )
        .await;
    devices[0]
        .api(
            "POST",
            "/api/sessions/movements",
            Some(json!({
                "movementType": "petty_expense",
                "amountCents": 700,
                "sessionKey": s1_key,
                "terminalId": "term_1",
                "categoryKey": category_key(CashMovementType::PettyExpense, "Tea and refreshments")
            })),
        )
        .await;

    // Device 2 offline actions
    devices[1]
        .api(
            "POST",
            "/api/billing/sales",
            Some(json!({
                "staff": { "cashierName": "D2" },
                "items": [{ "productKey": prod_key, "sellingPriceCents": 500, "quantity": 2, "discountCents": 0, "sourceType": "retail" }],
                "payment": { "paymentMethod": "online", "isCredit": false, "amountReceivedCents": 1000 },
                "shopProfileSnapshot": { "tradingName": "Conv Shared Shop" }
            })),
        )
        .await;
    let d2_cash_sale = devices[1]
        .api(
            "POST",
            "/api/billing/sales",
            Some(json!({
                "staff": { "cashierName": "D2" },
                "items": [{ "productKey": prod_key, "sellingPriceCents": 500, "quantity": 2, "discountCents": 0, "sourceType": "retail" }],
                "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 1000 },
                "shopProfileSnapshot": { "tradingName": "Conv Shared Shop" }
            })),
        )
        .await;
    let d2_cash_inv = d2_cash_sale["data"]["invoice"]["key"]
        .as_str()
        .expect("key")
        .to_string();
    devices[1]
        .api(
            "POST",
            "/api/billing/credit-notes",
            Some(json!({
                "invoiceKey": d2_cash_inv,
                "returnedItems": [{ "productKey": prod_key, "quantity": 1, "condition": "resalable", "reason": "defective" }],
                "paymentMethod": "cash"
            })),
        )
        .await;
    devices[1]
        .api(
            "POST",
            "/api/billing/sales",
            Some(json!({
                "customer": { "customerKey": cust_key },
                "staff": { "cashierName": "D2" },
                "items": [{ "productKey": prod_key, "sellingPriceCents": 500, "quantity": 8, "discountCents": 0, "sourceType": "retail" }],
                "payment": { "paymentMethod": "cash", "isCredit": true, "amountReceivedCents": 1000, "dueDate": "2099-01-01" },
                "shopProfileSnapshot": { "tradingName": "Conv Shared Shop" }
            })),
        )
        .await;
    devices[1]
        .api(
            "POST",
            "/api/sessions/movements",
            Some(json!({
                "movementType": "owner_withdrawal",
                "amountCents": 400,
                "sessionKey": s1_key,
                "terminalId": "term_2",
                "payeeName": "Owner",
                "managerPin": "1234",
                "categoryKey": category_key(CashMovementType::OwnerWithdrawal, "Personal use")
            })),
        )
        .await;
    devices[1]
        .api(
            "POST",
            "/api/sessions/movements",
            Some(json!({
                "movementType": "cash_drop",
                "amountCents": 600,
                "sessionKey": s1_key,
                "terminalId": "term_2",
                "categoryKey": category_key(CashMovementType::CashDrop, "To safe")
            })),
        )
        .await;

    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await;
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in &mut devices {
            pull_all(&cloud, d).await;
        }
    }
    assert!(cloud_conflicts(&cloud, "LWW_LOSER").await.is_empty());

    // Step C: Device 3 joins via bootstrap snapshot, then S1 closes + between-shift movement
    let mut d3 = Device::new(33, &cloud).await;
    d3.sync(
        "POST",
        "/api/sync/enable",
        Some(json!({ "tenantId": TENANT, "deviceId": d3.id })),
    )
    .await;
    reserve_blocks(&cloud, &d3).await;
    bootstrap(&cloud, &mut d3).await;
    devices.push(d3);

    assert_all_devices_match(&devices).await;
    let d3_drawer = table_rows(devices[2].pool(), "drawer_state").await;
    assert_eq!(d3_drawer[0]["key"], "shop");
    assert_eq!(d3_drawer[0]["current_balance_cents"], 11800);
    let d3_ses = &table_rows(devices[2].pool(), "register_sessions").await[0];
    assert_eq!(d3_ses["total_cash_sales_cents"], 4000);
    assert_eq!(d3_ses["total_card_sales_cents"], 1500);
    assert_eq!(d3_ses["total_online_sales_cents"], 1000);
    assert_eq!(d3_ses["total_credit_sales_cents"], 3000);
    assert_eq!(d3_ses["total_cash_refunds_cents"], 500);
    assert_eq!(d3_ses["total_pay_in_cents"], 0);
    assert_eq!(d3_ses["total_pay_out_cents"], 700);
    assert_eq!(d3_ses["total_owner_withdrawals_cents"], 400);
    assert_eq!(d3_ses["total_cash_drops_cents"], 600);

    // Close S1 on Device 2 -> sync -> between-shift petty_expense 400 on Device 1 -> sync
    devices[1]
        .api(
            "POST",
            &format!("/api/sessions/{s1_key}/close"),
            Some(json!({
                "countedCashCents": 11800,
                "closingFloatCents": 5000,
                "cashDropToSafeCents": 6800
            })),
        )
        .await;
    push_all(&cloud, &devices[1]).await;
    wait_for_cloud_quiet(&cloud).await;
    for d in &mut devices {
        pull_all(&cloud, d).await;
    }
    devices[0]
        .api(
            "POST",
            "/api/sessions/movements",
            Some(json!({
                "movementType": "petty_expense",
                "amountCents": 400,
                "sessionKey": null,
                "terminalId": "term_1",
                "categoryKey": category_key(CashMovementType::PettyExpense, "Tea and refreshments")
            })),
        )
        .await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    for d in &mut devices {
        pull_all(&cloud, d).await;
    }
    assert_all_devices_match(&devices).await;
    for d in &devices {
        assert_eq!(
            table_rows(d.pool(), "drawer_state").await[0]["current_balance_cents"],
            4600
        );
    }

    // Step D: Drop remaining 4600 via shift S2, switch to Personal mode, open personal shift
    let s2 = devices[2]
        .api(
            "POST",
            "/api/sessions/open",
            Some(json!({ "terminalId": "term_3", "openingFloatCents": 4600 })),
        )
        .await;
    let s2_key = s2["data"]["key"].as_str().expect("key").to_string();
    devices[2]
        .api(
            "POST",
            &format!("/api/sessions/{s2_key}/close"),
            Some(json!({
                "countedCashCents": 4600,
                "closingFloatCents": 0,
                "cashDropToSafeCents": 4600
            })),
        )
        .await;
    push_all(&cloud, &devices[2]).await;
    wait_for_cloud_quiet(&cloud).await;
    for d in &mut devices {
        pull_all(&cloud, d).await;
    }

    devices[0]
        .api(
            "PUT",
            "/api/settings/shop-profile",
            Some(json!({ "allowSharedUserSessions": false })),
        )
        .await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    for d in &mut devices {
        pull_all(&cloud, d).await;
    }

    let c1_tok = user_token(&devices[0].app.config, "usr_c1", Role::Staff, "Cashier One");
    let (status, open_personal) = call(
        &devices[0].app.router,
        "POST",
        "/api/sessions/open",
        &c1_tok,
        Some(json!({ "terminalId": "term_1", "openingFloatCents": 2000 })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "personal open: {open_personal}"
    );

    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    for d in &mut devices {
        pull_all(&cloud, d).await;
    }
    assert_all_devices_match(&devices).await;

    for d in &devices {
        let drawers = table_rows(d.pool(), "drawer_state").await;
        let shop = drawers
            .iter()
            .find(|r| r["key"] == "shop")
            .expect("shop drawer");
        let personal = drawers
            .iter()
            .find(|r| r["key"] == "usr_c1")
            .expect("usr_c1 drawer");
        assert_eq!(shop["current_balance_cents"], 0);
        assert_eq!(personal["current_balance_cents"], 2000);
    }

    let mongo_shop = with_tenant(Tenant::id(TENANT).expect("tid"), async {
        cloud
            .mongo()
            .collection::<Document>("drawer_state")
            .find_one(doc! { "tenant_id": TENANT, "key": "shop" })
            .await
    })
    .await
    .expect("query shop drawer")
    .expect("shop drawer in mongo");
    assert_eq!(mongo_shop.get_i64("current_balance_cents").ok(), Some(0));

    let mongo_personal = cloud
        .mongo()
        .collection::<Document>("drawer_state")
        .find_one(doc! { "tenant_id": TENANT, "key": "usr_c1" })
        .await
        .expect("query personal drawer")
        .expect("usr_c1 drawer in mongo");
    assert_eq!(
        mongo_personal.get_i64("current_balance_cents").ok(),
        Some(2000)
    );

    let mongo_profile = cloud
        .mongo()
        .collection::<Document>("shop_profiles")
        .find_one(doc! { "tenant_id": TENANT, "key": SHOP_PROFILE_KEY })
        .await
        .expect("query profile")
        .expect("shop profile in mongo");
    assert_eq!(
        mongo_profile.get_bool("allow_shared_user_sessions").ok(),
        Some(false)
    );

    cloud.mongo().drop().await.ok();
}

#[tokio::test]
async fn concurrent_offline_shared_shift_opens_converge_deterministically() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;

    let mut devices = vec![Device::new(51, &cloud).await, Device::new(52, &cloud).await];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
        reserve_blocks(&cloud, d).await;
    }

    ensure_shop_profile(&devices[0].app.db_handle, Some("Dual Conv Shop"))
        .await
        .expect("shop profile");
    devices[0]
        .api(
            "PUT",
            "/api/settings/shop-profile",
            Some(json!({ "allowSharedUserSessions": true })),
        )
        .await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    pull_all(&cloud, &mut devices[1]).await;

    let add_cat = category_key(CashMovementType::AddCash, "Change float top-up");
    // Device 1 opens slightly earlier offline
    let ses1 = devices[0]
        .api(
            "POST",
            "/api/sessions/open",
            Some(json!({ "terminalId": "term_1", "openingFloatCents": 10000 })),
        )
        .await;
    let ses1_key = ses1["data"]["key"].as_str().expect("key").to_string();
    devices[0]
        .api(
            "POST",
            "/api/sessions/movements",
            Some(json!({
                "movementType": "add_cash",
                "amountCents": 1000,
                "sessionKey": ses1_key,
                "terminalId": "term_1",
                "categoryKey": add_cat,
            })),
        )
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(60)).await;

    // Device 2 opens slightly later offline
    let ses2 = devices[1]
        .api(
            "POST",
            "/api/sessions/open",
            Some(json!({ "terminalId": "term_2", "openingFloatCents": 8000 })),
        )
        .await;
    let ses2_key = ses2["data"]["key"].as_str().expect("key").to_string();
    devices[1]
        .api(
            "POST",
            "/api/sessions/movements",
            Some(json!({
                "movementType": "add_cash",
                "amountCents": 500,
                "sessionKey": ses2_key,
                "terminalId": "term_2",
                "categoryKey": add_cat,
            })),
        )
        .await;

    // Push later shift first, then earlier shift, then drive to quiescence
    push_all(&cloud, &devices[1]).await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    for d in &mut devices {
        pull_all(&cloud, d).await;
    }

    let mut d3 = Device::new(53, &cloud).await;
    d3.sync(
        "POST",
        "/api/sync/enable",
        Some(json!({ "tenantId": TENANT, "deviceId": d3.id })),
    )
    .await;
    reserve_blocks(&cloud, &d3).await;
    bootstrap(&cloud, &mut d3).await;
    devices.push(d3);

    assert_all_devices_match(&devices).await;
    for d in &devices {
        assert_eq!(
            keys(d.pool(), "register_sessions").await.len(),
            2,
            "both active shifts kept on {}",
            d.id
        );
        let ses_nums: std::collections::BTreeSet<String> =
            sqlx::query_scalar("SELECT session_number FROM register_sessions")
                .fetch_all(d.pool())
                .await
                .expect("session numbers")
                .into_iter()
                .collect();
        assert_eq!(ses_nums.len(), 2, "session numbers stayed unique");
        assert_eq!(
            table_rows(d.pool(), "drawer_state").await[0]["current_balance_cents"],
            11000,
            "shop drawer must track earliest canonical session {ses1_key}"
        );
    }

    let cloud_dups = cloud_conflicts(&cloud, "UNIQUE_VIOLATION").await;
    assert!(
        cloud_dups.iter().any(|c| {
            c.get_document("detail")
                .ok()
                .and_then(|d| d.get_str("reason").ok())
                == Some("MULTIPLE_ACTIVE_SHARED_SESSIONS")
                && c.get_document("detail")
                    .ok()
                    .and_then(|d| d.get_str("canonicalKey").ok())
                    == Some(ses1_key.as_str())
        }),
        "missing cloud MULTIPLE_ACTIVE_SHARED_SESSIONS conflict: {cloud_dups:?}"
    );

    let mongo_drawer = cloud
        .mongo()
        .collection::<Document>("drawer_state")
        .find_one(doc! { "tenant_id": TENANT, "key": "shop" })
        .await
        .expect("find drawer")
        .expect("drawer in mongo");
    assert_eq!(
        mongo_drawer.get_i64("current_balance_cents").ok(),
        Some(11000)
    );

    cloud.mongo().drop().await.ok();
}
