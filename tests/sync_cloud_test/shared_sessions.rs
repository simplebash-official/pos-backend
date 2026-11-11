//! Cloud Sync v2 tests for shop profiles, drawer state, shared register
//! sessions, cash movements, and ledger-derived recomputation.

use axum::http::StatusCode;
use futures_util::TryStreamExt;
use mongodb::bson::{Bson, Document, doc, oid::ObjectId};
use serde_json::{Value, json};
use simplebash_pos_backend::core::tenancy::{Tenant, with_tenant};
use simplebash_pos_backend::domain::cash_categories::category_key;
use simplebash_pos_backend::domain::sessions::CashMovementType;
use simplebash_pos_backend::modules::cash_categories::service as cash_categories_service;
use simplebash_pos_backend::modules::cash_settings::service as cash_settings_service;
use simplebash_pos_backend::modules::settings::service::{SHOP_PROFILE_KEY, ensure_shop_profile};
use simplebash_pos_backend::modules::sync::apply_mongo::test_support::hydrate_for_test;

use crate::common;
use crate::helpers::{
    SERIAL, acks, admin_token, call, device_token, drop_db, push, record, reid, seed_tenant, start,
    statuses, text, wait_for,
};

struct SessionSeed {
    shop_profile: Value,
    drawer_state: Value,
    session: Value,
    movement: Value,
    category: Value,
    product: Value,
    customer: Value,
    invoice: Value,
    payment: Value,
}

async fn find_hydrated(
    app: &common::TestApp,
    tid: &str,
    coll_name: &str,
    resource: &str,
    filter: Document,
) -> Value {
    let mut full_filter = filter;
    full_filter.insert("tenant_id", tid);
    let doc = app
        .mongo()
        .collection::<Document>(coll_name)
        .find_one(full_filter)
        .await
        .expect("mongo find_one succeeds")
        .unwrap_or_else(|| panic!("missing {coll_name} doc for {tid}"));
    with_tenant(
        Tenant::id(tid).expect("valid tenant id"),
        hydrate_for_test(&app.db_handle, resource, doc),
    )
    .await
    .expect("hydrate_for_test succeeds")
}

async fn seed_shared_session_tenant(app: &common::TestApp, tid: &str) -> SessionSeed {
    let base = seed_tenant(app, tid).await;
    with_tenant(Tenant::id(tid).expect("valid tenant id"), async {
        ensure_shop_profile(&app.db_handle, Some("Seed Shared Shop")).await?;
        cash_categories_service::ensure_system_categories(&app.db_handle).await?;
        cash_settings_service::ensure_settings(&app.db_handle).await
    })
    .await
    .expect("seed settings and cash categories");

    let admin = admin_token(app, tid);
    let (status, body) = call(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(&admin),
        Some(json!({
            "tradingName": "Shared Sync Mart",
            "addressLines": ["10 Galle Rd", "Colombo 03"],
            "allowSharedUserSessions": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "shop profile update: {body}");

    let (status, body) = call(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(&admin),
        Some(json!({
            "terminalId": "term_seed",
            "openingFloatCents": 10000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "open session: {body}");
    let session_key = text(&body["data"], "/key");

    let (status, body) = call(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(&admin),
        Some(json!({
            "movementType": "add_cash",
            "amountCents": 2500,
            "sessionKey": session_key,
            "terminalId": "term_seed",
            "categoryKey": category_key(CashMovementType::AddCash, "Change float top-up")
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create movement: {body}");
    let movement_key = text(&body["data"], "/key");

    let (status, sale_body) = call(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(&admin),
        Some(json!({
            "staff": { "cashierName": "Seed Cashier" },
            "items": [{
                "productKey": base.product["key"],
                "sellingPriceCents": 2000,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "payment": {
                "paymentMethod": "cash",
                "isCredit": false,
                "amountReceivedCents": 2000
            },
            "shopProfileSnapshot": { "tradingName": "Shared Sync Mart" }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "seed sale: {sale_body}");
    let invoice_key = text(&sale_body["data"]["invoice"], "/key");

    let shop_profile = find_hydrated(
        app,
        tid,
        "shop_profiles",
        "shopProfiles",
        doc! { "key": SHOP_PROFILE_KEY },
    )
    .await;
    let drawer_state = find_hydrated(
        app,
        tid,
        "drawer_state",
        "drawerState",
        doc! { "key": "shop" },
    )
    .await;
    let session = find_hydrated(
        app,
        tid,
        "register_sessions",
        "registerSessions",
        doc! { "key": &session_key },
    )
    .await;
    let movement = find_hydrated(
        app,
        tid,
        "cash_movements",
        "cashMovements",
        doc! { "key": &movement_key },
    )
    .await;
    let invoice = find_hydrated(
        app,
        tid,
        "invoices",
        "invoices",
        doc! { "key": &invoice_key },
    )
    .await;
    let payment = find_hydrated(
        app,
        tid,
        "payments",
        "payments",
        doc! { "invoice_key": &invoice_key },
    )
    .await;

    SessionSeed {
        shop_profile,
        drawer_state,
        session,
        movement,
        category: base.category,
        product: base.product,
        customer: base.customer,
        invoice,
        payment,
    }
}

async fn pull_snapshot_items(app: &common::TestApp, token: &str) -> Vec<Value> {
    let mut items = Vec::new();
    let mut page: Option<String> = None;
    loop {
        let uri = match &page {
            Some(p) => format!("/api/sync/snapshot?limit=200&page={p}"),
            None => "/api/sync/snapshot?limit=200".to_string(),
        };
        let (status, body) = call(&app.router, "GET", &uri, Some(token), None).await;
        assert_eq!(status, StatusCode::OK, "snapshot: {body}");
        items.extend(
            body["data"]["changes"]
                .as_array()
                .expect("changes array")
                .iter()
                .cloned(),
        );
        match body["data"]["nextPage"].as_str() {
            Some(next) => page = Some(next.to_string()),
            None => return items,
        }
    }
}

#[tokio::test]
async fn shop_profile_drawer_session_and_movement_sync_and_web_edits() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;
    let seed = seed_shared_session_tenant(&app, "tnt_ss_src1").await;

    let tid = "tnt_ss_codec";
    let mut stale_drawer = reid(&seed.drawer_state);
    stale_drawer["currentBalanceCents"] = json!(0);

    let (status, body) = push(
        &app,
        tid,
        "dev_sc1",
        "batch-sc1",
        vec![
            record("shopProfiles", &reid(&seed.shop_profile), "dev_sc1"),
            record("categories", &seed.category, "dev_sc1"),
            record("customers", &reid(&seed.customer), "dev_sc1"),
            record("products", &reid(&seed.product), "dev_sc1"),
            record("registerSessions", &reid(&seed.session), "dev_sc1"),
            record("drawerState", &stale_drawer, "dev_sc1"),
            record("cashMovements", &reid(&seed.movement), "dev_sc1"),
            record("invoices", &reid(&seed.invoice), "dev_sc1"),
            record("payments", &reid(&seed.payment), "dev_sc1"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "push: {body}");
    assert!(
        statuses(&body).iter().all(|s| s == "applied"),
        "expected all applied: {body}"
    );

    let sess_key = text(&seed.session, "/key");
    let stored_session = app
        .mongo()
        .collection::<Document>("register_sessions")
        .find_one(doc! { "tenant_id": tid, "key": &sess_key })
        .await
        .expect("find session")
        .expect("session exists");
    assert!(
        matches!(stored_session.get("opened_at"), Some(Bson::DateTime(_))),
        "opened_at must be BSON DateTime, got {:?}",
        stored_session.get("opened_at")
    );
    assert_eq!(
        stored_session.get_i64("total_cash_sales_cents").ok(),
        Some(2000)
    );
    assert_eq!(
        stored_session.get_i64("total_pay_in_cents").ok(),
        Some(2500)
    );
    assert!(stored_session.get("expected_cash_cents").is_none());

    let stored_drawer = app
        .mongo()
        .collection::<Document>("drawer_state")
        .find_one(doc! { "tenant_id": tid, "key": "shop" })
        .await
        .expect("find drawer")
        .expect("drawer exists");
    assert_eq!(
        stored_drawer.get_i64("current_balance_cents").ok(),
        Some(14500)
    );

    let stored_profile = app
        .mongo()
        .collection::<Document>("shop_profiles")
        .find_one(doc! { "tenant_id": tid, "key": SHOP_PROFILE_KEY })
        .await
        .expect("find shop profile")
        .expect("shop profile exists");
    assert_eq!(
        stored_profile.get_bool("allow_shared_user_sessions").ok(),
        Some(true)
    );

    let dev2 = device_token(&app, tid, "dev_sc2").await;
    let snapshot = pull_snapshot_items(&app, &dev2).await;
    for res in [
        "shopProfiles",
        "drawerState",
        "registerSessions",
        "cashMovements",
    ] {
        assert!(
            snapshot.iter().any(|c| c["resource"] == res),
            "snapshot missing {res}: {snapshot:?}"
        );
    }
    let snap_profile = snapshot
        .iter()
        .find(|c| c["resource"] == "shopProfiles")
        .expect("shopProfiles item");
    assert_eq!(snap_profile["payload"]["allowSharedUserSessions"], true);
    assert_eq!(
        snap_profile["payload"]["addressLines"],
        json!(["10 Galle Rd", "Colombo 03"])
    );
    let snap_session = snapshot
        .iter()
        .find(|c| c["resource"] == "registerSessions")
        .expect("registerSessions item");
    assert!(snap_session["payload"]["openedAt"].is_string());
    assert_eq!(snap_session["payload"]["totalCashSalesCents"], 2000);
    assert_eq!(snap_session["payload"]["totalPayInCents"], 2500);
    assert!(snap_session["payload"].get("_id").is_none());

    let dev1 = device_token(&app, tid, "dev_sc1").await;
    let baseline = wait_for(&app, &dev1, 0, |_| true).await;
    let cursor = baseline["nextSeq"].as_i64().expect("nextSeq integer");

    let admin = admin_token(&app, tid);
    let (status, body) = call(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(&admin),
        Some(json!({
            "tradingName": "Web Renamed Shop",
            "addressLines": ["99 Union Pl"],
            "allowSharedUserSessions": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "web update shop profile: {body}");

    let (status, close_body) = call(
        &app.router,
        "POST",
        &format!("/api/sessions/{sess_key}/close"),
        Some(&admin),
        Some(json!({
            "countedCashCents": 14500,
            "closingFloatCents": 5000,
            "cashDropToSafeCents": 9500
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "web close session: {close_body}");

    let feed = wait_for(&app, &dev1, cursor, |changes| {
        changes.iter().any(|c| {
            c["resource"] == "shopProfiles" && c["payload"]["tradingName"] == "Web Renamed Shop"
        }) && changes.iter().any(|c| {
            c["resource"] == "registerSessions"
                && c["key"] == sess_key
                && c["payload"]["status"] == "closed"
        }) && changes.iter().any(|c| {
            c["resource"] == "drawerState"
                && c["key"] == "shop"
                && c["payload"]["currentBalanceCents"] == 5000
        })
    })
    .await;
    let closed_sess = feed["changes"]
        .as_array()
        .expect("changes")
        .iter()
        .rev()
        .find(|c| c["resource"] == "registerSessions" && c["key"] == sess_key)
        .expect("closed session in feed");
    assert_eq!(closed_sess["payload"]["totalCashDropsCents"], 0);
    assert_eq!(closed_sess["payload"]["cashDropToSafeCents"], 9500);
    assert_eq!(closed_sess["payload"]["expectedCashCents"], 14500);
    assert_eq!(closed_sess["payload"]["cashVarianceCents"], 0);

    drop_db(&app).await;
}

#[tokio::test]
async fn multi_device_sales_refunds_credit_movements_recompute_without_spurious_lww() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    let seed = seed_shared_session_tenant(&app, "tnt_ss_src2").await;

    let tid = "tnt_ss_multi";
    let base = chrono::DateTime::parse_from_rfc3339(&text(&seed.session, "/updatedAt"))
        .expect("valid rfc3339");
    let at = |secs: i64| (base + chrono::Duration::seconds(secs)).to_rfc3339();
    let sess_key = text(&seed.session, "/key");

    let mut clean_sess = reid(&seed.session);
    clean_sess["openingFloatCents"] = json!(10000);
    clean_sess["totalCashSalesCents"] = json!(0);
    clean_sess["totalCardSalesCents"] = json!(0);
    clean_sess["totalOnlineSalesCents"] = json!(0);
    clean_sess["totalCreditSalesCents"] = json!(0);
    clean_sess["totalCashRefundsCents"] = json!(0);
    clean_sess["totalPayInCents"] = json!(0);
    clean_sess["totalPayOutCents"] = json!(0);
    clean_sess["totalOwnerWithdrawalsCents"] = json!(0);
    clean_sess["totalCashDropsCents"] = json!(0);
    clean_sess["hasLateMovements"] = json!(false);
    clean_sess["updatedAt"] = json!(at(10));

    let mut drawer_zero = reid(&seed.drawer_state);
    drawer_zero["currentBalanceCents"] = json!(0);
    drawer_zero["updatedAt"] = json!(at(10));

    let mut pay_cash = reid(&seed.payment);
    pay_cash["key"] = json!("pay_sc_cash1");
    pay_cash["amountCents"] = json!(2000);
    pay_cash["paymentMethod"] = json!("cash");
    pay_cash["sessionKey"] = json!(sess_key);
    pay_cash["notes"] = json!(null);

    let mut pay_card = pay_cash.clone();
    pay_card["id"] = json!(ObjectId::new().to_hex());
    pay_card["key"] = json!("pay_sc_card1");
    pay_card["amountCents"] = json!(1500);
    pay_card["paymentMethod"] = json!("card");

    let mut pay_online = pay_cash.clone();
    pay_online["id"] = json!(ObjectId::new().to_hex());
    pay_online["key"] = json!("pay_sc_online1");
    pay_online["amountCents"] = json!(800);
    pay_online["paymentMethod"] = json!("online");

    let mut cust = reid(&seed.customer);
    cust["key"] = json!("cus_sc_1");
    let mut inv_cash = reid(&seed.invoice);
    inv_cash["customerKey"] = json!("cus_sc_1");
    inv_cash["sessionKey"] = json!(sess_key);

    let (status, body) = push(
        &app,
        tid,
        "dev_m1",
        "b-m1",
        vec![
            record("shopProfiles", &reid(&seed.shop_profile), "dev_m1"),
            record("categories", &seed.category, "dev_m1"),
            record("customers", &cust, "dev_m1"),
            record("products", &reid(&seed.product), "dev_m1"),
            record("registerSessions", &clean_sess, "dev_m1"),
            record("drawerState", &drawer_zero, "dev_m1"),
            record("invoices", &inv_cash, "dev_m1"),
            record("payments", &pay_cash, "dev_m1"),
            record("payments", &pay_card, "dev_m1"),
            record("payments", &pay_online, "dev_m1"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "dev_m1 push: {body}");
    assert!(statuses(&body).iter().all(|s| s == "applied"), "{body}");

    // Device 2 pushes older session DTO with stale derived totals plus ledger events
    let mut older_sess_derived_only = clean_sess.clone();
    older_sess_derived_only["updatedAt"] = json!(at(5));
    older_sess_derived_only["totalCashSalesCents"] = json!(999_999);
    older_sess_derived_only["totalPayInCents"] = json!(888_888);

    let mut inv_credit = reid(&inv_cash);
    inv_credit["key"] = json!("inv_sc_credit");
    inv_credit["invoiceNumber"] = json!("INV-CREDIT-1");
    inv_credit["isCredit"] = json!(true);
    inv_credit["status"] = json!("partially_paid");
    inv_credit["totalCents"] = json!(4000);
    inv_credit["amountReceivedCents"] = json!(1000);
    inv_credit["sessionKey"] = json!(sess_key);

    let mut pay_deposit = pay_cash.clone();
    pay_deposit["id"] = json!(ObjectId::new().to_hex());
    pay_deposit["key"] = json!("pay_sc_deposit");
    pay_deposit["invoiceKey"] = json!("inv_sc_credit");
    pay_deposit["amountCents"] = json!(1000);
    pay_deposit["paymentMethod"] = json!("cash");

    let mut pay_refund = pay_cash.clone();
    pay_refund["id"] = json!(ObjectId::new().to_hex());
    pay_refund["key"] = json!("pay_sc_refund");
    pay_refund["amountCents"] = json!(-500);
    pay_refund["notes"] = json!("Credit note refund");

    let mut pay_void_rev = pay_cash.clone();
    pay_void_rev["id"] = json!(ObjectId::new().to_hex());
    pay_void_rev["key"] = json!("pay_sc_void_rev");
    pay_void_rev["amountCents"] = json!(1000);
    pay_void_rev["notes"] = json!("Reversal of credit note CN-1");

    let mut pay_exch_diff = pay_cash.clone();
    pay_exch_diff["id"] = json!(ObjectId::new().to_hex());
    pay_exch_diff["key"] = json!("pay_sc_exch_diff");
    pay_exch_diff["amountCents"] = json!(300);
    pay_exch_diff["notes"] = json!("Payment for exchange/credit note difference CN-2");

    let make_mov =
        |k: &str, num: &str, mtype: &str, dir: &str, amt: i64, rev: Option<&str>, late: bool| {
            let mut m = reid(&seed.movement);
            m["key"] = json!(k);
            m["movementNumber"] = json!(num);
            m["movementType"] = json!(mtype);
            m["direction"] = json!(dir);
            m["amountCents"] = json!(amt);
            m["sessionKey"] = json!(sess_key);
            m["reversesKey"] = json!(rev);
            m["recordedLate"] = json!(late);
            m["updatedAt"] = json!(at(20));
            m
        };

    let mov_add = make_mov("mov_sc_add", "CSH-1", "add_cash", "in", 1200, None, false);
    let mov_add_rev = make_mov(
        "mov_sc_add_rev",
        "CSH-2",
        "add_cash",
        "out",
        1200,
        Some("mov_sc_add"),
        false,
    );
    let mov_petty = make_mov(
        "mov_sc_petty",
        "CSH-3",
        "petty_expense",
        "out",
        700,
        None,
        false,
    );
    let mov_owner = make_mov(
        "mov_sc_owner",
        "CSH-4",
        "owner_withdrawal",
        "out",
        400,
        None,
        false,
    );
    let mov_drop = make_mov("mov_sc_drop", "CSH-5", "cash_drop", "out", 600, None, false);
    let mov_late = make_mov(
        "mov_sc_late",
        "CSH-6",
        "rent_and_bills",
        "out",
        100,
        None,
        true,
    );

    let (status, body) = push(
        &app,
        tid,
        "dev_m2",
        "b-m2",
        vec![
            record("registerSessions", &older_sess_derived_only, "dev_m2"),
            record("invoices", &inv_credit, "dev_m2"),
            record("payments", &pay_deposit, "dev_m2"),
            record("payments", &pay_refund, "dev_m2"),
            record("payments", &pay_void_rev, "dev_m2"),
            record("payments", &pay_exch_diff, "dev_m2"),
            record("cashMovements", &mov_add, "dev_m2"),
            record("cashMovements", &mov_add_rev, "dev_m2"),
            record("cashMovements", &mov_petty, "dev_m2"),
            record("cashMovements", &mov_owner, "dev_m2"),
            record("cashMovements", &mov_drop, "dev_m2"),
            record("cashMovements", &mov_late, "dev_m2"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "dev_m2 push: {body}");
    let m2_statuses = statuses(&body);
    assert_eq!(
        m2_statuses[0], "duplicate",
        "older session with derived-only differences must return duplicate without conflict: {body}"
    );
    assert!(
        m2_statuses[1..].iter().all(|s| s == "applied"),
        "ledger rows from dev_m2 must apply: {body}"
    );

    let lww_conflicts = app
        .mongo()
        .collection::<Document>("sync_conflicts")
        .count_documents(doc! { "tenant_id": tid, "kind": "LWW_LOSER" })
        .await
        .expect("count lww conflicts");
    assert_eq!(lww_conflicts, 0);

    let sess_doc = app
        .mongo()
        .collection::<Document>("register_sessions")
        .find_one(doc! { "tenant_id": tid, "key": &sess_key })
        .await
        .expect("find session")
        .expect("session exists");
    assert_eq!(sess_doc.get_i64("total_cash_sales_cents").ok(), Some(3000));
    assert_eq!(sess_doc.get_i64("total_card_sales_cents").ok(), Some(1500));
    assert_eq!(sess_doc.get_i64("total_online_sales_cents").ok(), Some(800));
    assert_eq!(
        sess_doc.get_i64("total_credit_sales_cents").ok(),
        Some(3000)
    );
    assert_eq!(sess_doc.get_i64("total_cash_refunds_cents").ok(), Some(500));
    assert_eq!(sess_doc.get_i64("total_pay_in_cents").ok(), Some(0));
    assert_eq!(sess_doc.get_i64("total_pay_out_cents").ok(), Some(800));
    assert_eq!(
        sess_doc.get_i64("total_owner_withdrawals_cents").ok(),
        Some(400)
    );
    assert_eq!(sess_doc.get_i64("total_cash_drops_cents").ok(), Some(600));
    assert_eq!(sess_doc.get_bool("has_late_movements").ok(), Some(true));

    let drawer_doc = app
        .mongo()
        .collection::<Document>("drawer_state")
        .find_one(doc! { "tenant_id": tid, "key": "shop" })
        .await
        .expect("find drawer")
        .expect("drawer exists");
    assert_eq!(
        drawer_doc.get_i64("current_balance_cents").ok(),
        Some(10700)
    );

    drop_db(&app).await;
}

#[tokio::test]
async fn offline_dual_open_shared_sessions_records_unique_violation_and_uses_canonical() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    let seed = seed_shared_session_tenant(&app, "tnt_ss_src3").await;

    let tid = "tnt_ss_dual";
    let base = chrono::DateTime::parse_from_rfc3339(&text(&seed.session, "/openedAt"))
        .expect("valid openedAt");
    let at = |secs: i64| (base + chrono::Duration::seconds(secs)).to_rfc3339();

    let mut sess_b = reid(&seed.session);
    sess_b["key"] = json!("ses_dual_b");
    sess_b["sessionNumber"] = json!("SES-DUAL-B");
    sess_b["openedAt"] = json!(at(20));
    sess_b["createdAt"] = json!(at(20));
    sess_b["updatedAt"] = json!(at(20));
    sess_b["openingFloatCents"] = json!(8000);
    sess_b["totalCashSalesCents"] = json!(0);
    sess_b["totalPayInCents"] = json!(0);

    let mut mov_b = reid(&seed.movement);
    mov_b["key"] = json!("mov_dual_b");
    mov_b["movementNumber"] = json!("CSH-DUAL-B");
    mov_b["sessionKey"] = json!("ses_dual_b");
    mov_b["amountCents"] = json!(500);
    mov_b["updatedAt"] = json!(at(21));

    let mut sess_a = reid(&seed.session);
    sess_a["key"] = json!("ses_dual_a");
    sess_a["sessionNumber"] = json!("SES-DUAL-A");
    sess_a["openedAt"] = json!(at(10));
    sess_a["createdAt"] = json!(at(10));
    sess_a["updatedAt"] = json!(at(10));
    sess_a["openingFloatCents"] = json!(10000);
    sess_a["totalCashSalesCents"] = json!(0);
    sess_a["totalPayInCents"] = json!(0);

    let mut mov_a = reid(&seed.movement);
    mov_a["key"] = json!("mov_dual_a");
    mov_a["movementNumber"] = json!("CSH-DUAL-A");
    mov_a["sessionKey"] = json!("ses_dual_a");
    mov_a["amountCents"] = json!(1000);
    mov_a["updatedAt"] = json!(at(11));

    let (status, body) = push(
        &app,
        tid,
        "dev_d1",
        "b-d1",
        vec![
            record("shopProfiles", &reid(&seed.shop_profile), "dev_d1"),
            record("drawerState", &reid(&seed.drawer_state), "dev_d1"),
            record("registerSessions", &sess_b, "dev_d1"),
            record("cashMovements", &mov_b, "dev_d1"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "push d1: {body}");
    assert!(statuses(&body).iter().all(|s| s == "applied"), "{body}");

    let (status, body) = push(
        &app,
        tid,
        "dev_d2",
        "b-d2",
        vec![
            record("registerSessions", &sess_a, "dev_d2"),
            record("cashMovements", &mov_a, "dev_d2"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "push d2: {body}");
    assert_eq!(statuses(&body), ["applied", "applied"], "{body}");
    assert_eq!(acks(&body)[0]["reason"], Value::Null);

    let active_count = app
        .mongo()
        .collection::<Document>("register_sessions")
        .count_documents(doc! { "tenant_id": tid, "status": "open" })
        .await
        .expect("count active sessions");
    assert_eq!(active_count, 2, "both offline-opened shifts stay stored");

    let conflicts: Vec<Document> = app
        .mongo()
        .collection::<Document>("sync_conflicts")
        .find(doc! { "tenant_id": tid, "kind": "UNIQUE_VIOLATION" })
        .await
        .expect("find conflicts")
        .try_collect()
        .await
        .expect("collect conflicts");
    assert_eq!(conflicts.len(), 1);
    let detail = conflicts[0]
        .get_document("detail")
        .expect("conflict detail");
    assert_eq!(
        detail.get_str("reason").ok(),
        Some("MULTIPLE_ACTIVE_SHARED_SESSIONS")
    );
    assert_eq!(detail.get_str("canonicalKey").ok(), Some("ses_dual_a"));
    let active_keys: Vec<&str> = detail
        .get_array("activeSessionKeys")
        .expect("activeSessionKeys")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(active_keys, vec!["ses_dual_a", "ses_dual_b"]);

    let drawer = app
        .mongo()
        .collection::<Document>("drawer_state")
        .find_one(doc! { "tenant_id": tid, "key": "shop" })
        .await
        .expect("find drawer")
        .expect("drawer exists");
    assert_eq!(
        drawer.get_i64("current_balance_cents").ok(),
        Some(11000),
        "shop drawer follows earliest canonical active shift ses_dual_a (10000 + 1000)"
    );

    drop_db(&app).await;
}
