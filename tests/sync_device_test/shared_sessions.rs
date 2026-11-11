//! Device-side (SQLite) Sync v2 tests for shared shop sessions, shop profiles,
//! drawer state, cash movements, offline blocks, capture triggers, and derived recompute.

use axum::http::StatusCode;
use serde_json::{Value, json};
use simplebash_pos_backend::domain::cash_categories::category_key;
use simplebash_pos_backend::domain::sessions::CashMovementType;
use simplebash_pos_backend::modules::settings::service::{SHOP_PROFILE_KEY, ensure_shop_profile};

use crate::helpers::{Device, as_pulled, assert_same_data, table_rows};

async fn ack_outbox(dev: &Device) {
    dev.sync(
        "POST",
        "/api/sync/outbox/ack",
        Some(json!({ "upToSeq": 1_000_000 })),
    )
    .await;
}

async fn store_block(dev: &Device, name: &str, prefix: &str, start: u64, end: u64) {
    dev.sync(
        "POST",
        "/api/sync/blocks",
        Some(json!({
            "name": name,
            "prefix": prefix,
            "padding": 6,
            "start": start,
            "end": end,
            "expiresAt": "2099-01-01T00:00:00Z"
        })),
    )
    .await;
}

#[tokio::test]
async fn session_and_cash_movement_blocks_and_offline_fallback() {
    let dev = Device::new().await;
    dev.sync(
        "POST",
        "/api/sync/enable",
        Some(json!({ "deviceId": "dev_blk001" })),
    )
    .await;
    ensure_shop_profile(&dev.app.db_handle, Some("Block Shop"))
        .await
        .expect("shop profile initializes");

    store_block(&dev, "session", "SES-", 100, 101).await;
    // Desktop agent reserves under camelCase "cashMovement"; backend uses "cash_movement".
    store_block(&dev, "cashMovement", "CSH-", 200, 201).await;

    let open1 = dev
        .api(
            "POST",
            "/api/sessions/open",
            Some(json!({ "terminalId": "term_1", "openingFloatCents": 5000 })),
        )
        .await;
    let ses1_key = open1["data"]["key"].as_str().expect("key").to_string();
    assert_eq!(open1["data"]["sessionNumber"], "SES-000100");

    let add_cat = category_key(CashMovementType::AddCash, "Change float top-up");
    let mut numbers = Vec::new();
    for amount in [1000, 500, 300] {
        let mov = dev
            .api(
                "POST",
                "/api/sessions/movements",
                Some(json!({
                    "movementType": "add_cash",
                    "amountCents": amount,
                    "sessionKey": ses1_key,
                    "terminalId": "term_1",
                    "categoryKey": add_cat,
                })),
            )
            .await;
        numbers.push(
            mov["data"]["movementNumber"]
                .as_str()
                .expect("movementNumber")
                .to_string(),
        );
    }
    assert_eq!(numbers[0], "CSH-000200");
    assert_eq!(numbers[1], "CSH-000201");
    assert_eq!(numbers[2], "CSH-Dblk0-000001");

    dev.api(
        "POST",
        &format!("/api/sessions/{ses1_key}/close"),
        Some(json!({
            "countedCashCents": 6800,
            "closingFloatCents": 0,
            "cashDropToSafeCents": 6800
        })),
    )
    .await;

    let open2 = dev
        .api(
            "POST",
            "/api/sessions/open",
            Some(json!({ "terminalId": "term_1", "openingFloatCents": 0 })),
        )
        .await;
    let ses2_key = open2["data"]["key"].as_str().expect("key").to_string();
    assert_eq!(open2["data"]["sessionNumber"], "SES-000101");

    dev.api(
        "POST",
        &format!("/api/sessions/{ses2_key}/close"),
        Some(json!({
            "countedCashCents": 0,
            "closingFloatCents": 0,
            "cashDropToSafeCents": 0
        })),
    )
    .await;

    let open3 = dev
        .api(
            "POST",
            "/api/sessions/open",
            Some(json!({ "terminalId": "term_1", "openingFloatCents": 0 })),
        )
        .await;
    assert_eq!(open3["data"]["sessionNumber"], "SES-Dblk0-000001");
}

#[tokio::test]
async fn shop_profile_drawer_session_and_movement_sync_derived_sqlite_and_late_movements() {
    let a = Device::new().await;
    let b = Device::new().await;
    for (dev, id) in [(&a, "dev_sqlite_a"), (&b, "dev_sqlite_b")] {
        dev.sync("POST", "/api/sync/enable", Some(json!({ "deviceId": id })))
            .await;
        ack_outbox(dev).await;
    }

    ensure_shop_profile(&a.app.db_handle, Some("Initial Shop"))
        .await
        .expect("shop profile initializes");
    a.api(
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({
            "tradingName": "Shared SQLite Shop",
            "addressLines": ["No 5, Kings St", "Kandy"],
            "allowSharedUserSessions": true
        })),
    )
    .await;

    a.api(
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
    let mgr_login = a
        .api(
            "POST",
            "/api/auth/login",
            Some(json!({ "username": "mgr_pin", "password": "Password123!" })),
        )
        .await;
    let mgr_token = mgr_login["data"]["token"].as_str().expect("token");
    let (pin_status, pin_body) = a
        .call(
            "POST",
            "/api/auth/set-pin",
            Some(json!({ "pin": "1234" })),
            Some(mgr_token),
        )
        .await;
    assert_eq!(pin_status, StatusCode::OK, "set-pin: {pin_body}");

    let cat = a
        .api(
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": "Parts", "icon": "Box", "color": "blue" })),
        )
        .await;
    let cat_key = cat["data"]["key"].as_str().expect("key").to_string();
    let sub = a
        .api(
            "POST",
            &format!("/api/inventory/categories/{cat_key}/subcategories"),
            Some(json!({ "name": "Cables" })),
        )
        .await;
    let sub_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("key")
        .to_string();
    let prod = a
        .api(
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": "USB Cable",
                "categoryKey": cat_key,
                "subcategoryKey": sub_key,
                "warranty": { "coverage": { "kind": "none" } },
                "minStockThreshold": 2,
                "openingStock": { "quantity": 20, "unitCostCents": 400, "sellingPriceCents": 1000 }
            })),
        )
        .await;
    let prod_key = prod["data"]["key"].as_str().expect("key").to_string();
    let cust = a
        .api(
            "POST",
            "/api/customers",
            Some(json!({ "name": "Kamal", "primaryPhone": "0771112223" })),
        )
        .await;
    let cust_key = cust["data"]["key"].as_str().expect("key").to_string();

    let open = a
        .api(
            "POST",
            "/api/sessions/open",
            Some(json!({ "terminalId": "term_a", "openingFloatCents": 10000 })),
        )
        .await;
    let ses_key = open["data"]["key"].as_str().expect("key").to_string();

    // Cash sale qty 2 @ 1000 = 2000; later return 1 -> 1000 refund
    let cash_sale = a
        .api(
            "POST",
            "/api/billing/sales",
            Some(json!({
                "staff": { "cashierName": "A" },
                "items": [{ "productKey": prod_key, "sellingPriceCents": 1000, "quantity": 2, "discountCents": 0, "sourceType": "retail" }],
                "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 2000 },
                "shopProfileSnapshot": { "tradingName": "Shared SQLite Shop" }
            })),
        )
        .await;
    let cash_inv_key = cash_sale["data"]["invoice"]["key"]
        .as_str()
        .expect("key")
        .to_string();

    a.api(
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "A" },
            "items": [{ "productKey": prod_key, "sellingPriceCents": 1000, "quantity": 2, "discountCents": 0, "sourceType": "retail" }],
            "payment": { "paymentMethod": "card", "isCredit": false, "amountReceivedCents": 2000 },
            "shopProfileSnapshot": { "tradingName": "Shared SQLite Shop" }
        })),
    )
    .await;
    a.api(
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "A" },
            "items": [{ "productKey": prod_key, "sellingPriceCents": 1000, "quantity": 1, "discountCents": 0, "sourceType": "retail" }],
            "payment": { "paymentMethod": "online", "isCredit": false, "amountReceivedCents": 1000 },
            "shopProfileSnapshot": { "tradingName": "Shared SQLite Shop" }
        })),
    )
    .await;
    a.api(
        "POST",
        "/api/billing/sales",
        Some(json!({
            "customer": { "customerKey": cust_key },
            "staff": { "cashierName": "A" },
            "items": [{ "productKey": prod_key, "sellingPriceCents": 1000, "quantity": 4, "discountCents": 0, "sourceType": "retail" }],
            "payment": { "paymentMethod": "cash", "isCredit": true, "amountReceivedCents": 1000, "dueDate": "2099-01-01" },
            "shopProfileSnapshot": { "tradingName": "Shared SQLite Shop" }
        })),
    )
    .await;
    a.api(
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": cash_inv_key,
            "returnedItems": [{ "productKey": prod_key, "quantity": 1, "condition": "resalable", "reason": "defective" }],
            "paymentMethod": "cash"
        })),
    )
    .await;

    let add_mov = a
        .api(
            "POST",
            "/api/sessions/movements",
            Some(json!({
                "movementType": "add_cash",
                "amountCents": 1200,
                "sessionKey": ses_key,
                "terminalId": "term_a",
                "categoryKey": category_key(CashMovementType::AddCash, "Change float top-up")
            })),
        )
        .await;
    let add_key = add_mov["data"]["key"].as_str().expect("key").to_string();
    a.api(
        "POST",
        &format!("/api/sessions/movements/{add_key}/reverse"),
        Some(json!({ "reason": "Undo duplicate float add" })),
    )
    .await;
    a.api(
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "petty_expense",
            "amountCents": 700,
            "sessionKey": ses_key,
            "terminalId": "term_a",
            "categoryKey": category_key(CashMovementType::PettyExpense, "Tea and refreshments")
        })),
    )
    .await;
    a.api(
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "owner_withdrawal",
            "amountCents": 400,
            "sessionKey": ses_key,
            "terminalId": "term_a",
            "payeeName": "Owner",
            "managerPin": "1234",
            "categoryKey": category_key(CashMovementType::OwnerWithdrawal, "Personal use")
        })),
    )
    .await;
    a.api(
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "cash_drop",
            "amountCents": 600,
            "sessionKey": ses_key,
            "terminalId": "term_a",
            "categoryKey": category_key(CashMovementType::CashDrop, "To safe")
        })),
    )
    .await;

    let outbox1 = a.drain_outbox().await;
    ack_outbox(&a).await;
    let mut changes1 = as_pulled(&outbox1);

    // Check boolean wire fidelity from clients_sqlite allowlist
    let profile_rec = changes1
        .iter()
        .find(|c| c["resource"] == "shopProfiles")
        .expect("shopProfiles outbox record");
    assert_eq!(
        profile_rec["payload"]["allowSharedUserSessions"],
        Value::Bool(true)
    );
    assert_eq!(
        profile_rec["payload"]["addressLines"],
        json!(["No 5, Kings St", "Kandy"])
    );
    let sess_rec = changes1
        .iter()
        .find(|c| c["resource"] == "registerSessions")
        .expect("registerSessions outbox record");
    assert_eq!(sess_rec["payload"]["hasLateMovements"], Value::Bool(false));
    let mov_rec = changes1
        .iter()
        .find(|c| c["resource"] == "cashMovements")
        .expect("cashMovements outbox record");
    assert_eq!(mov_rec["payload"]["recordedLate"], Value::Bool(false));

    // Corrupt derived values in transit to verify derived_sqlite recomputes from ledger
    for ch in &mut changes1 {
        match ch["resource"].as_str().expect("resource") {
            "registerSessions" => {
                ch["payload"]["totalCashSalesCents"] = json!(999_999);
                ch["payload"]["totalPayInCents"] = json!(888_888);
                ch["payload"]["expectedCashCents"] = json!(777_777);
            }
            "drawerState" => {
                ch["payload"]["currentBalanceCents"] = json!(1);
            }
            _ => {}
        }
    }

    b.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "incremental", "changes": changes1 })),
    )
    .await;
    assert_eq!(b.pending_out().await, 0);
    assert_same_data(&a, &b).await;

    let b_sessions = table_rows(b.pool(), "register_sessions").await;
    assert_eq!(b_sessions.len(), 1);
    assert_eq!(b_sessions[0]["total_cash_sales_cents"], 3000);
    assert_eq!(b_sessions[0]["total_card_sales_cents"], 2000);
    assert_eq!(b_sessions[0]["total_online_sales_cents"], 1000);
    assert_eq!(b_sessions[0]["total_credit_sales_cents"], 3000);
    assert_eq!(b_sessions[0]["total_cash_refunds_cents"], 1000);
    assert_eq!(b_sessions[0]["total_pay_in_cents"], 0);
    assert_eq!(b_sessions[0]["total_pay_out_cents"], 700);
    assert_eq!(b_sessions[0]["total_owner_withdrawals_cents"], 400);
    assert_eq!(b_sessions[0]["total_cash_drops_cents"], 600);
    assert!(b_sessions[0]["expected_cash_cents"].is_null());

    let b_drawers = table_rows(b.pool(), "drawer_state").await;
    assert_eq!(b_drawers[0]["key"], "shop");
    // 10000 + 3000 - 1000 - 700 - 400 - 600 = 10300
    assert_eq!(b_drawers[0]["current_balance_cents"], 10300);

    // Phase 2: close session with variance -300 and add between-shift movement
    a.api(
        "POST",
        &format!("/api/sessions/{ses_key}/close"),
        Some(json!({
            "countedCashCents": 10000,
            "closingFloatCents": 6000,
            "cashDropToSafeCents": 4000,
            "varianceReason": "Till short 300"
        })),
    )
    .await;
    a.api(
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "petty_expense",
            "amountCents": 500,
            "sessionKey": null,
            "terminalId": "term_a",
            "categoryKey": category_key(CashMovementType::PettyExpense, "Tea and refreshments")
        })),
    )
    .await;

    let changes2 = as_pulled(&a.drain_outbox().await);
    ack_outbox(&a).await;
    b.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "incremental", "changes": changes2 })),
    )
    .await;
    assert_same_data(&a, &b).await;

    let b_closed = &table_rows(b.pool(), "register_sessions").await[0];
    assert_eq!(b_closed["status"], "closed");
    assert_eq!(b_closed["expected_cash_cents"], 10300);
    assert_eq!(b_closed["cash_variance_cents"], -300);
    let b_drawer_closed = &table_rows(b.pool(), "drawer_state").await[0];
    assert_eq!(b_drawer_closed["current_balance_cents"], 5500);

    // Phase 3: apply a late movement recorded against the closed shift
    let mut late_mov = changes2
        .iter()
        .find(|c| c["resource"] == "cashMovements")
        .expect("movement in changes2")
        .clone();
    late_mov["key"] = json!("mov_late_sqlite");
    late_mov["payload"]["id"] = json!("mov_late_sqlite");
    late_mov["payload"]["key"] = json!("mov_late_sqlite");
    late_mov["payload"]["movementNumber"] = json!("CSH-LATE-1");
    late_mov["payload"]["movementType"] = json!("owner_withdrawal");
    late_mov["payload"]["amountCents"] = json!(300);
    late_mov["payload"]["sessionKey"] = json!(ses_key);
    late_mov["payload"]["recordedLate"] = json!(true);
    late_mov["payload"]["originalVarianceSessionKey"] = json!(ses_key);
    late_mov["seq"] = json!(999);

    b.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "incremental", "changes": [late_mov] })),
    )
    .await;
    let b_after_late = &table_rows(b.pool(), "register_sessions").await[0];
    assert_eq!(b_after_late["has_late_movements"], Value::Bool(true));
    assert_eq!(b_after_late["total_owner_withdrawals_cents"], 700);
    assert_eq!(b_after_late["expected_cash_cents"], 10000);
    assert_eq!(b_after_late["cash_variance_cents"], 0);
}

#[tokio::test]
async fn offline_dual_open_shared_sessions_records_sqlite_conflict_and_converges() {
    let a = Device::new().await;
    let b = Device::new().await;
    for (dev, id) in [(&a, "dev_dual_a"), (&b, "dev_dual_b")] {
        dev.sync("POST", "/api/sync/enable", Some(json!({ "deviceId": id })))
            .await;
        ack_outbox(dev).await;
    }

    ensure_shop_profile(&a.app.db_handle, Some("Dual Open Shop"))
        .await
        .expect("shop profile");
    a.api(
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": true })),
    )
    .await;
    let base_changes = as_pulled(&a.drain_outbox().await);
    ack_outbox(&a).await;
    b.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "incremental", "changes": base_changes })),
    )
    .await;

    let base = chrono::Utc::now();
    let t1 = (base + chrono::Duration::seconds(10)).to_rfc3339();
    let t2 = (base + chrono::Duration::seconds(20)).to_rfc3339();

    let sess_payload = |key: &str, num: &str, opened_at: &str, float_cents: i64, dev_id: &str| {
        json!({
            "resource": "registerSessions",
            "key": key,
            "op": "upsert",
            "version": 1,
            "updatedAt": opened_at,
            "deviceId": dev_id,
            "seq": 1,
            "payload": {
                "id": key,
                "key": key,
                "sessionNumber": num,
                "terminalId": "term_1",
                "cashierId": "admin",
                "cashierNameSnapshot": "Admin",
                "status": "open",
                "openedAt": opened_at,
                "openingFloatCents": float_cents,
                "totalCashSalesCents": 0,
                "totalCardSalesCents": 0,
                "totalOnlineSalesCents": 0,
                "totalCreditSalesCents": 0,
                "totalCashRefundsCents": 0,
                "totalPayInCents": 0,
                "totalPayOutCents": 0,
                "totalOwnerWithdrawalsCents": 0,
                "totalCashDropsCents": 0,
                "hasLateMovements": false,
                "version": 1,
                "createdAt": opened_at,
                "updatedAt": opened_at,
                "updatedByDevice": dev_id
            }
        })
    };
    let mov_payload = |key: &str, num: &str, ses: &str, ts: &str, amount: i64, dev_id: &str| {
        json!({
            "resource": "cashMovements",
            "key": key,
            "op": "upsert",
            "version": 1,
            "updatedAt": ts,
            "deviceId": dev_id,
            "seq": 2,
            "payload": {
                "id": key,
                "key": key,
                "movementNumber": num,
                "movementType": "add_cash",
                "direction": "in",
                "amountCents": amount,
                "sessionKey": ses,
                "terminalId": "term_1",
                "categoryKey": category_key(CashMovementType::AddCash, "Change float top-up"),
                "recordedByUserId": "admin",
                "recordedByNameSnapshot": "Admin",
                "recordedLate": false,
                "version": 1,
                "createdAt": ts,
                "updatedAt": ts,
                "updatedByDevice": dev_id
            }
        })
    };
    let drawer_payload = json!({
        "resource": "drawerState",
        "key": "shop",
        "op": "upsert",
        "version": 1,
        "updatedAt": t1,
        "deviceId": "dev_dual_a",
        "seq": 3,
        "payload": {
            "id": "shop",
            "key": "shop",
            "userKey": "shop",
            "currentBalanceCents": 8500,
            "lastClosingFloatCents": 0,
            "version": 1,
            "createdAt": t1,
            "updatedAt": t1,
            "updatedByDevice": "dev_dual_a"
        }
    });

    let ch_a = sess_payload("ses_sql_b", "SES-B", &t2, 8000, "dev_dual_b");
    let ch_mov_a = mov_payload("mov_sql_b", "CSH-B", "ses_sql_b", &t2, 500, "dev_dual_b");
    let ch_b = sess_payload("ses_sql_a", "SES-A", &t1, 10000, "dev_dual_a");
    let ch_mov_b = mov_payload("mov_sql_a", "CSH-A", "ses_sql_a", &t1, 1000, "dev_dual_a");

    // Apply in opposite orders on `a` and `b`
    a.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "incremental", "changes": [drawer_payload.clone(), ch_a.clone(), ch_mov_a.clone(), ch_b.clone(), ch_mov_b.clone()] })),
    )
    .await;
    b.sync(
        "POST",
        "/api/sync/apply",
        Some(json!({ "mode": "incremental", "changes": [drawer_payload, ch_b, ch_mov_b, ch_a, ch_mov_a] })),
    )
    .await;

    assert_same_data(&a, &b).await;
    for dev in [&a, &b] {
        assert_eq!(table_rows(dev.pool(), "register_sessions").await.len(), 2);
        assert_eq!(
            table_rows(dev.pool(), "drawer_state").await[0]["current_balance_cents"],
            11000
        );
        let (status, conflicts) = dev
            .call(
                "GET",
                "/api/sync/conflicts?status=open",
                None,
                Some(&dev.agent),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "conflicts: {conflicts}");
        let list = conflicts["data"].as_array().expect("conflicts list");
        assert!(
            list.iter().any(|c| {
                c["kind"] == "UNIQUE_VIOLATION"
                    && c["detail"]["reason"] == "MULTIPLE_ACTIVE_SHARED_SESSIONS"
                    && c["detail"]["canonicalKey"] == "ses_sql_a"
                    && c["detail"]["activeSessionKeys"] == json!(["ses_sql_a", "ses_sql_b"])
            }),
            "missing MULTIPLE_ACTIVE_SHARED_SESSIONS conflict on device: {list:?}"
        );
        let profile_key = table_rows(dev.pool(), "shop_profiles").await[0]["key"]
            .as_str()
            .expect("key")
            .to_string();
        assert_eq!(profile_key, SHOP_PROFILE_KEY);
    }
}
