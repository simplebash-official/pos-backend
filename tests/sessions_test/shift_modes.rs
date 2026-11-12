//! Shift modes tests.

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;

use mongodb::bson::doc;
use serde_json::Value;
use serde_json::json;
use simplebash_pos_backend::clients::db::Db;
use simplebash_pos_backend::core::constants::roles;

use simplebash_pos_backend::domain::sessions::CashMovementType;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::modules::sessions::repository::sessions as sessions_repo;

use crate::common;
use crate::helpers::cat;
use crate::helpers::execute;
use crate::helpers::mint_token_for;
use crate::helpers::send_authed;
use crate::helpers::send_authed_with_device;

#[tokio::test]
async fn shift_follows_its_owner_and_staff_see_it_on_one_machine_sqlite() {
    let app = common::spawn_app_sqlite().await;

    // Seed Admin
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the awaited step succeeds");

    let admin_token = mint_token_for(
        &app.config,
        "admin",
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    // 1. Initial shop profile has allowSharedUserSessions: false
    let (status, profile_body) = send_authed(
        &app.router,
        "GET",
        "/api/settings/shop-profile",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(profile_body["data"]["allowSharedUserSessions"], false);
    assert_eq!(profile_body["data"]["allowStaffMultiDeviceSessions"], false);

    let (status, summary_body) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/active-summary",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {summary_body}");
    assert_eq!(summary_body["data"]["activeSessions"], json!([]));
    assert_eq!(summary_body["data"]["drawerBalanceTotalCents"], 0);

    // Mint tokens for Cashier 1 and Cashier 2
    let cashier1_token = mint_token_for(
        &app.config,
        "usr_cashier1",
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );
    let cashier2_token = mint_token_for(
        &app.config,
        "usr_cashier2",
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );

    // 2. In personal mode, Cashier 1 opens shift on term_1
    let (status, open_res) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({
            "terminalId": "term_1",
            "openingFloatCents": 10000
        })),
        &cashier1_token,
        Some("term_1"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {open_res}");
    let session_1_key = open_res["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // 3. Mid-shift guard: toggling allowSharedUserSessions while active shift exists fails
    let (status, conflict_res) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({
            "allowSharedUserSessions": true
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {conflict_res}");
    assert_eq!(conflict_res["code"], "ACTIVE_SESSIONS_EXIST");

    let (status, summary_body) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/active-summary",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {summary_body}");
    assert_eq!(
        summary_body["data"]["activeSessions"]
            .as_array()
            .expect("array")
            .len(),
        1
    );
    assert_eq!(summary_body["data"]["drawerBalanceTotalCents"], 10000);

    // 4. In Personal mode, a machine shows only the signed-in user's own shift:
    // a) Cashier 1 on term_1 resolves to session_1_key
    let (status, res) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier1_token,
        Some("term_1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["data"]["key"], session_1_key);

    // b) Cashier 2 on term_1 does not get Cashier 1's shift: he has none of his own
    let (status, res) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier2_token,
        Some("term_1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(res["data"].is_null(), "another user's shift leaked: {res}");

    // c) Staff away from the machine their shift was opened on see no shift
    let (status, res) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier1_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        res["data"].is_null(),
        "staff shift shown away from its machine: {res}"
    );

    // d) Closing status still blocks mode switching
    let (status, bc_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_1_key}/blind-count"),
        Some(json!({ "countedCashCents": 10000 })),
        &cashier1_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {bc_res}");
    assert_eq!(bc_res["data"]["status"], "closing");
    assert_eq!(bc_res["data"]["closingCashierId"], "usr_cashier1");

    let (status, conflict_res) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {conflict_res}");
    assert_eq!(conflict_res["code"], "ACTIVE_SESSIONS_EXIST");

    // 5. Close session 1 keeping 10,000 in drawer, verify DRAWER_BALANCE_NONZERO guard (2B)
    let (status, close_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_1_key}/close"),
        Some(json!({
            "countedCashCents": 10000,
            "closingFloatCents": 10000,
            "cashDropToSafeCents": 0
        })),
        &cashier1_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {close_res}");

    let (status, conflict_res) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {conflict_res}");
    assert_eq!(conflict_res["code"], "DRAWER_BALANCE_NONZERO");

    // 5b. Reopen session 1: reopened status also blocks mode switching
    let (status, reopen_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_1_key}/reopen"),
        Some(json!({ "reason": "Drop remaining float to safe before mode switch" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {reopen_res}");
    assert_eq!(reopen_res["data"]["status"], "reopened");

    let (status, conflict_res) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {conflict_res}");
    assert_eq!(conflict_res["code"], "ACTIVE_SESSIONS_EXIST");

    // Drop entire balance to safe so drawer balance total is 0
    let (status, close_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_1_key}/close"),
        Some(json!({
            "countedCashCents": 10000,
            "closingFloatCents": 0,
            "cashDropToSafeCents": 10000
        })),
        &cashier1_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {close_res}");

    // 6. With active sessions = 0 and drawer balance = 0, switching to SharedShop succeeds
    let (status, update_res) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({
            "allowSharedUserSessions": true
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {update_res}");
    assert_eq!(update_res["data"]["allowSharedUserSessions"], true);

    // 7. In SharedShop Mode, Cashier 2 opens shift on term_2
    let (status, open_res) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({
            "terminalId": "term_2",
            "openingFloatCents": 5000
        })),
        &cashier2_token,
        Some("term_2"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {open_res}");
    let session_2_key = open_res["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // Shared drawer state belongs to "shop"
    let (status, drawer_res) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/drawer-state",
        None,
        &cashier1_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {drawer_res}");
    assert_eq!(drawer_res["data"]["userKey"], "shop");
    assert_eq!(drawer_res["data"]["currentBalanceCents"], 5000);

    // 8. Mid-shift guard in reverse: switching back while session 2 is active fails
    let (status, conflict_res) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({
            "allowSharedUserSessions": false
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {conflict_res}");
    assert_eq!(conflict_res["code"], "ACTIVE_SESSIONS_EXIST");

    // 9. SharedShop rules: all authenticated staff across any machine see the shared shift
    let (status, res) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier1_token,
        Some("term_1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["data"]["key"], session_2_key);

    let (status, res) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier1_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["data"]["key"], session_2_key);

    let (status, res) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier2_token,
        Some("term_9"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["data"]["key"], session_2_key);

    // Another cashier cannot open a second concurrent shop shift
    let (status, second_open_res) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({
            "terminalId": "term_1",
            "openingFloatCents": 0
        })),
        &cashier1_token,
        Some("term_1"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {second_open_res}");
    assert_eq!(second_open_res["code"], "SESSION_ALREADY_OPEN");

    // 10. Cashier 1 (Role::Staff, not the opener) can close the shared shift, dropping cash to 0
    let (status, close_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_2_key}/close"),
        Some(json!({
            "countedCashCents": 5000,
            "closingFloatCents": 0,
            "cashDropToSafeCents": 5000
        })),
        &cashier1_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {close_res}");

    // 11. Now switching back to personal mode succeeds
    let (status, update_res) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({
            "allowSharedUserSessions": false
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {update_res}");
    assert_eq!(update_res["data"]["allowSharedUserSessions"], false);
}

fn is_object_id_hex(value: &str) -> bool {
    value.len() == 24 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

async fn login_on_device(
    app: &common::TestApp,
    username: &str,
    device: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .header("x-device-id", device)
        .body(Body::from(
            json!({ "username": username, "password": "Password123!" }).to_string(),
        ))
        .expect("the request is well formed");
    execute(&app.router, request).await
}

async fn create_staff_user(
    app: &common::TestApp,
    admin_token: &str,
    name: &str,
    prefix: &str,
) -> (String, String) {
    let username = format!(
        "{prefix}_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": name,
            "username": username,
            "password": "Password123!",
            "role": "staff"
        })),
        admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Create staff body: {body}");
    let id = body["data"]["id"]
        .as_str()
        .expect("staff user has id")
        .to_string();
    (id, username)
}

#[tokio::test]
async fn shared_shop_mid_count_lock_and_cross_device_lifecycle_sqlite() {
    let app = common::spawn_app_sqlite().await;

    // 1. Fresh SQLite shop_profiles row has a 24-hex ObjectId ready for Sync v2
    if let Db::Sqlite(pool) = &app.db_handle {
        let profile_id: String = sqlx::query_scalar("SELECT id FROM shop_profiles LIMIT 1")
            .fetch_one(pool)
            .await
            .expect("shop_profiles row initialized");
        assert!(
            is_object_id_hex(&profile_id),
            "expected 24-hex shop_profiles.id, got {profile_id}"
        );
    }

    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("admin seeds");
    let admin_token = mint_token_for(
        &app.config,
        "admin",
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    let (staff_a_id, staff_a_username) =
        create_staff_user(&app, &admin_token, "Staff A", "stfa").await;
    let (staff_b_id, staff_b_username) =
        create_staff_user(&app, &admin_token, "Staff B", "stfb").await;
    let (_staff_c_id, staff_c_username) =
        create_staff_user(&app, &admin_token, "Staff C", "stfc").await;

    // 2. Enable SharedShop mode
    let (status, body) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    // 3. Staff A logs in on term_1 and opens the shop shift (20,000 float)
    let (status, login_a) = login_on_device(&app, &staff_a_username, "term_1").await;
    assert_eq!(status, StatusCode::OK, "Login A body: {login_a}");
    let staff_a_token = login_a["data"]["token"]
        .as_str()
        .expect("token a")
        .to_string();

    let (status, open_res) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": "term_1", "openingFloatCents": 20_000 })),
        &staff_a_token,
        Some("term_1"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Open body: {open_res}");
    let session_key = open_res["data"]["key"]
        .as_str()
        .expect("session key")
        .to_string();

    if let Db::Sqlite(pool) = &app.db_handle {
        let (drawer_id, drawer_key): (String, String) =
            sqlx::query_as("SELECT id, key FROM drawer_state WHERE user_key = 'shop'")
                .fetch_one(pool)
                .await
                .expect("shop drawer row exists");
        assert!(
            is_object_id_hex(&drawer_id),
            "expected 24-hex drawer_state.id, got {drawer_id}"
        );
        assert_eq!(drawer_key, "shop");
    }

    // 4. Staff B signs in on term_2 (permitted in SharedShop even with an open shop shift)
    let (status, login_b) = login_on_device(&app, &staff_b_username, "term_2").await;
    assert_eq!(status, StatusCode::OK, "Login B body: {login_b}");
    let staff_b_token = login_b["data"]["token"]
        .as_str()
        .expect("token b")
        .to_string();

    let (status, current_b) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &staff_b_token,
        Some("term_2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current_b["data"]["key"], session_key);

    let (status, drawer_b) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/drawer-state",
        None,
        &staff_b_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {drawer_b}");
    assert_eq!(drawer_b["data"]["userKey"], "shop");
    assert_eq!(drawer_b["data"]["currentBalanceCents"], 20_000);

    // 5. Seed product + customer, then Staff B records a cash product sale, credit sale, and add_cash
    let (status, cat_res) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "Lock Test Cat", "icon": "box", "color": "blue" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Category body: {cat_res}");
    let cat_key = cat_res["data"]["key"].as_str().expect("category key");

    let (status, sub_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/inventory/categories/{cat_key}/subcategories"),
        Some(json!({ "name": "Widgets" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Subcat body: {sub_res}");
    let subcat_key = sub_res["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("subcategory key");

    let (status, prod_res) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Lock Widget",
            "categoryKey": cat_key,
            "subcategoryKey": subcat_key,
            "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 5, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 1
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Product body: {prod_res}");
    let prod_key = prod_res["data"]["key"]
        .as_str()
        .expect("product key")
        .to_string();

    let (status, cust_res) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({ "name": "Shared Lock Customer", "primaryPhone": "0775551234" })),
        &staff_b_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Customer body: {cust_res}");
    let cust_key = cust_res["data"]["key"]
        .as_str()
        .expect("customer key")
        .to_string();

    // Cash product sale (sessionKey omitted -> shared shift auto-resolved)
    let (status, sale_res) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Staff B" },
            "items": [{
                "productKey": prod_key,
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
            "shopProfileSnapshot": { "tradingName": "Test Shop" }
        })),
        &staff_b_token,
        Some("term_2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Cash sale body: {sale_res}");
    assert_eq!(sale_res["data"]["invoice"]["sessionKey"], session_key);
    let product_inv_key = sale_res["data"]["invoice"]["key"]
        .as_str()
        .expect("product invoice key")
        .to_string();

    // Credit invoice for testing payment lock during Closing
    let (status, credit_res) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Staff B" },
            "customer": { "customerKey": cust_key },
            "items": [{
                "sourceType": "retail",
                "name": "Credit Service",
                "unitPriceCents": 10000,
                "quantity": 1,
                "discountCents": 0
            }],
            "payment": {
                "paymentMethod": "cash",
                "isCredit": true,
                "amountReceivedCents": 0,
                "dueDate": "2099-01-01"
            },
            "shopProfileSnapshot": { "tradingName": "Test Shop" }
        })),
        &staff_b_token,
        Some("term_2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Credit sale body: {credit_res}");
    let credit_inv_key = credit_res["data"]["invoice"]["key"]
        .as_str()
        .expect("credit invoice key")
        .to_string();

    // Cash movement linked to the shared shift by Staff B
    let (status, mov_res) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "add_cash",
            "amountCents": 2000,
            "sessionKey": session_key,
            "terminalId": "term_2",
            "categoryKey": cat(CashMovementType::AddCash, "Change float top-up")
        })),
        &staff_b_token,
        Some("term_2"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Movement body: {mov_res}");
    assert_eq!(mov_res["data"]["sessionKey"], session_key);

    // Shop drawer now holds 20,000 + 2,000 + 2,000 = 24,000 cents
    let (status, drawer_after) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/drawer-state",
        None,
        &staff_a_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(drawer_after["data"]["currentBalanceCents"], 24_000);

    // 6. Staff B submits blind count -> status == closing, closingCashier* populated
    let (status, bc_res) = send_authed_with_device(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/blind-count"),
        Some(json!({ "countedCashCents": 24_000 })),
        &staff_b_token,
        Some("term_2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Blind count body: {bc_res}");
    assert_eq!(bc_res["data"]["status"], "closing");
    assert_eq!(bc_res["data"]["closingCashierId"], staff_b_id);
    assert_eq!(bc_res["data"]["closingCashierNameSnapshot"], "Staff B");

    // 7. Mid-count lock (409 SESSION_CLOSING) blocks sales, invoice payments, credit notes,
    //    cash movements (explicit or omitted sessionKey), and calc/cash-movement.
    let (status, body) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Staff A" },
            "items": [{
                "sourceType": "retail",
                "name": "Blocked Sale",
                "unitPriceCents": 1000,
                "quantity": 1,
                "discountCents": 0
            }],
            "payment": {
                "paymentMethod": "cash",
                "isCredit": false,
                "amountReceivedCents": 1000
            },
            "shopProfileSnapshot": { "tradingName": "Test Shop" }
        })),
        &staff_a_token,
        Some("term_1"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Blocked sale body: {body}");
    assert_eq!(body["code"], "SESSION_CLOSING");

    let (status, body) = send_authed_with_device(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{credit_inv_key}/payments"),
        Some(json!({ "amountCents": 5000, "paymentMethod": "cash" })),
        &staff_a_token,
        Some("term_1"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Blocked payment body: {body}");
    assert_eq!(body["code"], "SESSION_CLOSING");

    let (status, body) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": product_inv_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "condition": "resalable"
            }],
            "paymentMethod": "cash"
        })),
        &staff_a_token,
        Some("term_1"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "Blocked credit note body: {body}"
    );
    assert_eq!(body["code"], "SESSION_CLOSING");

    for payload in [
        json!({
            "movementType": "petty_expense",
            "amountCents": 500,
            "sessionKey": session_key,
            "terminalId": "term_1",
            "categoryKey": cat(CashMovementType::PettyExpense, "Tea and refreshments")
        }),
        json!({
            "movementType": "petty_expense",
            "amountCents": 500,
            "terminalId": "term_1",
            "categoryKey": cat(CashMovementType::PettyExpense, "Tea and refreshments")
        }),
    ] {
        let (status, body) = send_authed_with_device(
            &app.router,
            "POST",
            "/api/sessions/movements",
            Some(payload),
            &staff_a_token,
            Some("term_1"),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "Blocked movement body: {body}"
        );
        assert_eq!(body["code"], "SESSION_CLOSING");
    }

    for skey in [json!(session_key), Value::Null] {
        let (status, body) = send_authed_with_device(
            &app.router,
            "POST",
            "/api/calc/cash-movement",
            Some(json!({
                "movementType": "petty_expense",
                "amountCents": 500,
                "categoryKey": cat(CashMovementType::PettyExpense, "Tea and refreshments"),
                "sessionKey": skey,
                "terminalId": "term_1",
                "payeeKey": null,
                "payeeName": "",
                "period": null,
                "settlementAmountsCents": [],
                "pinEntered": false,
                "pinVerified": false,
                "overrideReasonEntered": false,
                "confirmDuplicate": false
            })),
            &staff_a_token,
            Some("term_1"),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "Blocked calc body: {body}");
        assert_eq!(body["code"], "SESSION_CLOSING");
    }

    // 8. Staff A cancels close -> clears closingCashier* and unlocks register mutations
    let (status, cancel_res) = send_authed_with_device(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/cancel-close"),
        Some(json!({})),
        &staff_a_token,
        Some("term_1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Cancel close body: {cancel_res}");
    assert_eq!(cancel_res["data"]["status"], "open");
    assert!(cancel_res["data"]["closingCashierId"].is_null());
    assert!(cancel_res["data"]["closingCashierNameSnapshot"].is_null());

    // Invoice cash payment now succeeds -> drawer goes to 29,000
    let (status, pay_res) = send_authed_with_device(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{credit_inv_key}/payments"),
        Some(json!({ "amountCents": 5000, "paymentMethod": "cash" })),
        &staff_a_token,
        Some("term_1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Unblocked payment body: {pay_res}");

    // 9. Optimistic concurrency check: stale version in update_session returns Ok(None)
    let mut stale = sessions_repo::find_session_by_key(&app.db_handle, &session_key)
        .await
        .expect("query succeeds")
        .expect("session found");
    stale.version -= 1;
    let conflict = sessions_repo::update_session(&app.db_handle, &stale)
        .await
        .expect("query succeeds");
    assert!(
        conflict.is_none(),
        "stale version must return None for VERSION_CONFLICT"
    );
    let _ = staff_a_id;

    // 10. Staff B (not opener) closes shift leaving 15,000 closing float in "shop" drawer
    let (status, close_res) = send_authed_with_device(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/close"),
        Some(json!({
            "countedCashCents": 29_000,
            "closingFloatCents": 15_000,
            "cashDropToSafeCents": 14_000
        })),
        &staff_b_token,
        Some("term_2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Close body: {close_res}");
    assert_eq!(close_res["data"]["status"], "closed");

    // 11. Staff C logs in on term_3 and opens next shift -> expects "shop" 15,000 float
    let (status, login_c) = login_on_device(&app, &staff_c_username, "term_3").await;
    assert_eq!(status, StatusCode::OK, "Login C body: {login_c}");
    let staff_c_token = login_c["data"]["token"]
        .as_str()
        .expect("token c")
        .to_string();

    let (status, next_open) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": "term_3", "openingFloatCents": 15_000 })),
        &staff_c_token,
        Some("term_3"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Next open body: {next_open}");
    assert_eq!(next_open["data"]["openingFloatCents"], 15_000);
    assert_eq!(next_open["data"]["openingDiscrepancyCents"], 0);
}

#[tokio::test]
async fn shared_shop_cross_user_and_2b_guards_mongo() {
    let app = common::spawn_app_isolated().await;

    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("admin seeds");
    let admin_token = mint_token_for(
        &app.config,
        "admin",
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let cashier1_token = mint_token_for(
        &app.config,
        "usr_cashier1",
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );
    let cashier2_token = mint_token_for(
        &app.config,
        "usr_cashier2",
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );

    // 1. shop_profiles exists initially on MongoDB with ObjectId _id and version >= 1
    let (status, profile_body) = send_authed(
        &app.router,
        "GET",
        "/api/settings/shop-profile",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Profile body: {profile_body}");
    assert_eq!(profile_body["data"]["allowSharedUserSessions"], false);
    assert_eq!(profile_body["data"]["allowStaffMultiDeviceSessions"], false);
    let profile_doc = app
        .mongo()
        .collection::<mongodb::bson::Document>("shop_profiles")
        .find_one(doc! { "key": "main" })
        .await
        .expect("query succeeds")
        .expect("shop_profiles document created");
    assert!(profile_doc.get_object_id("_id").is_ok());
    assert!(profile_doc.get_i64("version").expect("version") >= 1);

    // 2. Open personal shift, verify active-summary and 2B guards across open/closing/reopened + non-zero drawer
    let (status, open_res) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": "term_m1", "openingFloatCents": 10_000 })),
        &cashier1_token,
        Some("term_m1"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {open_res}");
    let session_1_key = open_res["data"]["key"]
        .as_str()
        .expect("session key")
        .to_string();

    let (status, summary) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/active-summary",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        summary["data"]["activeSessions"]
            .as_array()
            .expect("array")
            .len(),
        1
    );
    assert_eq!(summary["data"]["drawerBalanceTotalCents"], 10_000);

    // Close keeping 10,000 float -> DRAWER_BALANCE_NONZERO blocks mode switch
    let (status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_1_key}/close"),
        Some(json!({
            "countedCashCents": 10_000,
            "closingFloatCents": 10_000,
            "cashDropToSafeCents": 0
        })),
        &cashier1_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {body}");
    assert_eq!(body["code"], "DRAWER_BALANCE_NONZERO");

    // Reopen -> ACTIVE_SESSIONS_EXIST, then drop all cash to safe
    let (status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_1_key}/reopen"),
        Some(json!({ "reason": "Zero drawer before SharedShop" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "ACTIVE_SESSIONS_EXIST");

    let (status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_1_key}/close"),
        Some(json!({
            "countedCashCents": 10_000,
            "closingFloatCents": 0,
            "cashDropToSafeCents": 10_000
        })),
        &cashier1_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 3. Switch to SharedShop
    let (status, body) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    // 4. Cashier 1 opens shared shift; verify Mongo drawer_state sync fields & ObjectId
    let (status, open2) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": "term_m1", "openingFloatCents": 12_000 })),
        &cashier1_token,
        Some("term_m1"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {open2}");
    let session_2_key = open2["data"]["key"]
        .as_str()
        .expect("session key")
        .to_string();

    let drawer_doc = app
        .mongo()
        .collection::<mongodb::bson::Document>("drawer_state")
        .find_one(doc! { "user_key": "shop" })
        .await
        .expect("query succeeds")
        .expect("shop drawer exists on Mongo");
    assert!(drawer_doc.get_object_id("_id").is_ok());
    assert_eq!(drawer_doc.get_str("key").expect("key"), "shop");
    assert!(drawer_doc.get_i64("version").expect("version") >= 1);
    assert!(drawer_doc.get_datetime("created_at").is_ok());

    // 5. Cashier 2 on another terminal sees shared shift, enters Closing, locks sales/movements, cancels close, and closes
    let (status, curr) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier2_token,
        Some("term_m2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(curr["data"]["key"], session_2_key);

    let (status, bc) = send_authed_with_device(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_2_key}/blind-count"),
        Some(json!({ "countedCashCents": 12_000 })),
        &cashier2_token,
        Some("term_m2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {bc}");
    assert_eq!(bc["data"]["closingCashierId"], "usr_cashier2");

    let (status, locked_mov) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "petty_expense",
            "amountCents": 100,
            "terminalId": "term_m1",
            "categoryKey": cat(CashMovementType::PettyExpense, "Tea and refreshments")
        })),
        &cashier1_token,
        Some("term_m1"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {locked_mov}");
    assert_eq!(locked_mov["code"], "SESSION_CLOSING");

    let (status, cancel) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_2_key}/cancel-close"),
        Some(json!({})),
        &cashier1_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(cancel["data"]["closingCashierId"].is_null());

    let mut stale = sessions_repo::find_session_by_key(&app.db_handle, &session_2_key)
        .await
        .expect("query succeeds")
        .expect("session found");
    stale.version -= 1;
    assert!(
        sessions_repo::update_session(&app.db_handle, &stale)
            .await
            .expect("query succeeds")
            .is_none()
    );

    let (status, closed) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_2_key}/close"),
        Some(json!({
            "countedCashCents": 12_000,
            "closingFloatCents": 0,
            "cashDropToSafeCents": 12_000
        })),
        &cashier2_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {closed}");

    let (status, reset) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": false })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {reset}");

    let _ = app.mongo().drop().await;
}

#[tokio::test]
async fn reconciliation_exact_and_variance_calculation_on_blind_count() {
    let app = common::spawn_app().await;

    // Seed Admin
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the awaited step succeeds");

    let admin_token = mint_token_for(
        &app.config,
        "admin",
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    // Set Manager PIN
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/auth/set-pin",
        Some(json!({ "pin": "1234" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let staff_username = format!("stf_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let (status, staff_res) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Staff Alice",
            "username": staff_username,
            "password": "Password123!",
            "role": "staff",
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Staff body: {staff_res}");
    let staff_id = staff_res["data"]["id"]
        .as_str()
        .expect("the value is a string");

    let staff_token = mint_token_for(
        &app.config,
        staff_id,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );

    let term = format!("term_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    // 1. Open session with float Rs. 20.00 (2000 cents)
    let (status, open_res) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({
            "terminalId": term,
            "openingFloatCents": 2000
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {open_res}");
    let session_key = open_res["data"]["key"]
        .as_str()
        .expect("the value is a string");

    // Verify session initially has expectedCashCents = 2000
    let (status, current_res) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/{session_key}"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current_res["data"]["expectedCashCents"], 2000);
    assert_eq!(current_res["data"]["countedCashCents"], Value::Null);
    assert_eq!(current_res["data"]["cashVarianceCents"], Value::Null);

    // 2. Make cash sale of Rs. 4,446.12 (444612 cents)
    let (status, sale_res) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "sessionKey": session_key,
            "staff": { "cashierName": "Alice" },
            "items": [{
                "sourceType": "retail",
                "name": "Groceries",
                "unitPriceCents": 444612,
                "quantity": 1,
                "discountCents": 0
            }],
            "payment": { "paymentMethod": "cash", "isCredit": false },
            "shopProfileSnapshot": { "tradingName": "Test Shop" }
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {sale_res}");

    // Verify expectedCashCents is now 2000 + 444612 = 446612 cents
    let (status, after_sale_res) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/{session_key}"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after_sale_res["data"]["expectedCashCents"], 446612);

    // 3. Submit Blind Count of Rs. 4,446.12 (444612 cents) - exact match to user scenario
    let (status, count_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/blind-count"),
        Some(json!({
            "countedCashCents": 444612
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {count_res}");
    assert_eq!(count_res["data"]["status"], "closing");
    assert_eq!(count_res["data"]["countedCashCents"], 444612);
    // MUST calculate exact expected cash and non-zero variance!
    assert_eq!(count_res["data"]["expectedCashCents"], 446612);
    assert_eq!(count_res["data"]["cashVarianceCents"], -2000);

    // 4. Convert the -2000 cents shortage to an owner withdrawal
    let (status, convert_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/convert-variance"),
        Some(json!({
            "reason": "Owner took Rs. 20 for tea",
            "recipient": "Shop Owner",
            "managerPin": "1234"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {convert_res}");

    // Verify session now has variance cleared to 0 and expected cash reconciled to counted cash (444612)
    let (status, post_convert_res) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/{session_key}"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(post_convert_res["data"]["cashVarianceCents"], 0);
    assert_eq!(post_convert_res["data"]["totalOwnerWithdrawalsCents"], 2000);
    assert_eq!(post_convert_res["data"]["expectedCashCents"], 444612);

    // 5. Close the session cleanly
    let (status, close_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/close"),
        Some(json!({
            "countedCashCents": 444612,
            "closingFloatCents": 2000,
            "cashDropToSafeCents": 442612
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {close_res}");
    assert_eq!(close_res["data"]["status"], "closed");
    assert_eq!(close_res["data"]["expectedCashCents"], 444612);
    assert_eq!(close_res["data"]["cashVarianceCents"], 0);
}

#[tokio::test]
async fn test_cashier_name_resolved_when_token_lacks_name() {
    let app = common::spawn_app().await;

    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the awaited step succeeds");

    let admin_token = mint_token_for(
        &app.config,
        "admin",
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    let username = format!("kasun_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let (status, users_res) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Kasun Cashier",
            "username": username,
            "password": "Password123!",
            "role": "staff",
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {users_res}");
    let cashier_id = users_res["data"]["id"]
        .as_str()
        .expect("the value is a string");

    // Mint token for Kasun WITHOUT name claim to simulate older token or unpropagated name
    let claims = simplebash_pos_backend::core::middleware::auth::Claims {
        sub: cashier_id.to_string(),
        exp: 9_999_999_999,
        role: Some(Role::Staff),
        permissions: roles::default_permissions(Role::Staff)
            .iter()
            .map(|p| p.to_string())
            .collect(),
        tid: None,
        scope: None,
        did: None,
        name: None,
    };
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(app.config.jwt_secret.as_bytes()),
    )
    .expect("the token encodes");

    // Open session: should resolve Kasun's name via resolve_name()
    let terminal_id = format!(
        "term_kasun_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let (status, open_res) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({
            "terminalId": terminal_id,
            "openingFloatCents": 1000,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {open_res}");
    assert_eq!(open_res["data"]["cashierNameSnapshot"], "Kasun Cashier");

    // Fetch current session: should also have Kasun Cashier
    let (status, cur_res) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/current?terminal_id={terminal_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cur_res["data"]["cashierNameSnapshot"], "Kasun Cashier");
}

#[tokio::test]
async fn cancel_close_returns_a_counted_shift_to_open() {
    let app = common::spawn_app().await;

    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the awaited step succeeds");
    let admin_token = mint_token_for(
        &app.config,
        "admin",
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    let staff_username = format!("stf_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let (status, staff_res) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Staff Cara",
            "username": staff_username,
            "password": "Password123!",
            "role": "staff",
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Staff body: {staff_res}");
    let staff_token = mint_token_for(
        &app.config,
        staff_res["data"]["id"]
            .as_str()
            .expect("the value is a string"),
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );

    let term = format!("term_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let (status, open_res) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": term, "openingFloatCents": 2000 })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {open_res}");
    let key = open_res["data"]["key"]
        .as_str()
        .expect("the value is a string");
    let cancel_path = format!("/api/sessions/{key}/cancel-close");

    // Counting moves the shift to closing; cancelling puts it back and forgets the count.
    let (status, counted) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{key}/blind-count"),
        Some(json!({ "countedCashCents": 1500 })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {counted}");
    assert_eq!(counted["data"]["status"], "closing");

    let (status, cancelled) =
        send_authed(&app.router, "POST", &cancel_path, None, &staff_token).await;
    assert_eq!(status, StatusCode::OK, "Body: {cancelled}");
    assert_eq!(cancelled["data"]["status"], "open");
    assert_eq!(cancelled["data"]["countedCashCents"], Value::Null);
    assert_eq!(cancelled["data"]["cashVarianceCents"], Value::Null);

    // Cancelling again, on an open shift, changes nothing.
    let (status, again) = send_authed(&app.router, "POST", &cancel_path, None, &staff_token).await;
    assert_eq!(status, StatusCode::OK, "Body: {again}");
    assert_eq!(again["data"]["status"], "open");

    // The shift can be counted again from scratch.
    let (status, recounted) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{key}/blind-count"),
        Some(json!({ "countedCashCents": 2000 })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {recounted}");
    assert_eq!(recounted["data"]["status"], "closing");

    // A closed shift cannot be un-closed this way.
    let (status, closed) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{key}/close"),
        Some(json!({
            "countedCashCents": 2000,
            "closingFloatCents": 2000,
            "cashDropToSafeCents": 0
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {closed}");
    let (status, refused) =
        send_authed(&app.router, "POST", &cancel_path, None, &staff_token).await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {refused}");

    // Unknown shift.
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/nope/cancel-close",
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn run_staff_multi_device_policy_transitions(app: &common::TestApp) {
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("admin seeds");
    let admin_token = mint_token_for(
        &app.config,
        "admin",
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let staff_token = mint_token_for(
        &app.config,
        "usr_staff_policy",
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );

    // 1. Initial defaults: both flags false.
    let (status, initial) = send_authed(
        &app.router,
        "GET",
        "/api/settings/shop-profile",
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{initial}");
    assert_eq!(initial["data"]["allowSharedUserSessions"], false);
    assert_eq!(initial["data"]["allowStaffMultiDeviceSessions"], false);

    // 2. Active shift in open / closing / reopened blocks allowStaffMultiDeviceSessions changes
    // with 409 ACTIVE_SESSIONS_EXIST.
    let (status, open_res) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": "term_pol1", "openingFloatCents": 5_000 })),
        &staff_token,
        Some("term_pol1"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{open_res}");
    let session_key = open_res["data"]["key"]
        .as_str()
        .expect("session key")
        .to_string();

    let (status, open_conflict) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowStaffMultiDeviceSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{open_conflict}");
    assert_eq!(open_conflict["code"], "ACTIVE_SESSIONS_EXIST");

    let (status, blind_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/blind-count"),
        Some(json!({ "countedCashCents": 5_000 })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{blind_res}");
    assert_eq!(blind_res["data"]["status"], "closing");

    let (status, closing_conflict) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowStaffMultiDeviceSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{closing_conflict}");
    assert_eq!(closing_conflict["code"], "ACTIVE_SESSIONS_EXIST");

    // Close keeping 5,000 cents in drawer, reopen, and check reopened shift blocks toggle too.
    let (status, close1_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/close"),
        Some(json!({
            "countedCashCents": 5_000,
            "closingFloatCents": 5_000,
            "cashDropToSafeCents": 0
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{close1_res}");

    let (status, reopen_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/reopen"),
        Some(json!({ "reason": "Verify reopened guard" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reopen_res}");
    assert_eq!(reopen_res["data"]["status"], "reopened");

    let (status, reopened_conflict) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowStaffMultiDeviceSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{reopened_conflict}");
    assert_eq!(reopened_conflict["code"], "ACTIVE_SESSIONS_EXIST");

    // 3. Re-close keeping 5,000 cents in drawer (shifts = 0, drawer_state total = 5,000):
    // Option 1 allows toggling allowStaffMultiDeviceSessions BOTH ways inside Personal mode
    // WITHOUT checking DRAWER_BALANCE_NONZERO, while allowSharedUserSessions still requires 0.
    let (status, close2_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/close"),
        Some(json!({
            "countedCashCents": 5_000,
            "closingFloatCents": 5_000,
            "cashDropToSafeCents": 0
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{close2_res}");

    let (status, enable_ok) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowStaffMultiDeviceSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enable_ok}");
    assert_eq!(enable_ok["data"]["allowSharedUserSessions"], false);
    assert_eq!(enable_ok["data"]["allowStaffMultiDeviceSessions"], true);

    let (status, shared_drawer_conflict) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{shared_drawer_conflict}");
    assert_eq!(shared_drawer_conflict["code"], "DRAWER_BALANCE_NONZERO");

    let (status, disable_ok) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowStaffMultiDeviceSessions": false })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{disable_ok}");
    assert_eq!(disable_ok["data"]["allowStaffMultiDeviceSessions"], false);

    // 4. Open & close draining drawer to 0, test SharedShop validation rules & atomic transition.
    let (status, open2_res) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": "term_pol1", "openingFloatCents": 5_000 })),
        &staff_token,
        Some("term_pol1"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{open2_res}");
    let s2_key = open2_res["data"]["key"]
        .as_str()
        .expect("s2 key")
        .to_string();

    let (status, close3_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{s2_key}/close"),
        Some(json!({
            "countedCashCents": 5_000,
            "closingFloatCents": 0,
            "cashDropToSafeCents": 5_000
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{close3_res}");

    // Cannot change allowStaffMultiDeviceSessions when transitioning to SharedShop.
    let (status, bad_enter_shared) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({
            "allowSharedUserSessions": true,
            "allowStaffMultiDeviceSessions": true
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{bad_enter_shared}");

    let (status, enter_shared) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enter_shared}");
    assert_eq!(enter_shared["data"]["allowSharedUserSessions"], true);

    // Reject changing allowStaffMultiDeviceSessions while remaining in SharedShop.
    let (status, bad_in_shared) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowStaffMultiDeviceSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{bad_in_shared}");

    // Atomic SharedShop -> Personal with both flags in a single PUT succeeds.
    let (status, atomic_exit) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({
            "allowSharedUserSessions": false,
            "allowStaffMultiDeviceSessions": true
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{atomic_exit}");
    assert_eq!(atomic_exit["data"]["allowSharedUserSessions"], false);
    assert_eq!(atomic_exit["data"]["allowStaffMultiDeviceSessions"], true);

    // Switch back to SharedShop (leaving allowStaffMultiDeviceSessions unchanged = true), then
    // atomically switch SharedShop -> Personal setting allowStaffMultiDeviceSessions: false.
    let (status, reenter_shared) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowSharedUserSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reenter_shared}");

    let (status, bad_in_shared_disable) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowStaffMultiDeviceSessions": false })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{bad_in_shared_disable}");

    let (status, atomic_exit_false) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({
            "allowSharedUserSessions": false,
            "allowStaffMultiDeviceSessions": false
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{atomic_exit_false}");
    assert_eq!(atomic_exit_false["data"]["allowSharedUserSessions"], false);
    assert_eq!(
        atomic_exit_false["data"]["allowStaffMultiDeviceSessions"],
        false
    );
}

#[tokio::test]
async fn staff_multi_device_policy_transitions_sqlite() {
    run_staff_multi_device_policy_transitions(&common::spawn_app_sqlite().await).await;
}

#[tokio::test]
async fn staff_multi_device_policy_transitions_mongo() {
    let app = common::spawn_app_isolated().await;
    run_staff_multi_device_policy_transitions(&app).await;
    app.mongo().drop().await.ok();
}
