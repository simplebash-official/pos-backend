//! Shift modes tests.

use axum::http::StatusCode;

use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;

use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::helpers::{mint_token_for, send_authed, send_authed_with_device};

#[tokio::test]
async fn test_shared_vs_isolated_register_shift_mode_and_mid_shift_guard_sqlite() {
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

    // 2. In isolated mode, Cashier 1 opens shift on term_1
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

    // 3. Mid-shift guard: attempting to toggle allowSharedUserSessions while active shift exists fails
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

    // 4. Test resolution in Isolated Mode (allowSharedUserSessions == false):
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

    // b) Cashier 2 on term_1 resolves to term_1's session
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
    assert_eq!(res["data"]["key"], session_1_key);

    // c) Cashier 1 with NO terminal_id / device-id returns null in isolated mode
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
        "Expected null in isolated mode without terminal_id: {res}"
    );

    // 5. Close session 1
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

    // 6. With active sessions = 0, updating allowSharedUserSessions to true succeeds
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

    // 7. In Shared Mode, Cashier 2 opens shift on term_2
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

    // 8. Mid-shift guard in reverse: attempting to switch back to isolated mode while session 2 is active fails
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

    // 9. Test resolution in Shared Mode (allowSharedUserSessions == true):
    // a) Cashier 1 operating on term_2 resolves to term_2's active session
    let (status, res) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier1_token,
        Some("term_2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["data"]["key"], session_2_key);

    // b) Cashier 2 with NO device_id or unknown terminal falls back to Cashier 2's active session
    let (status, res) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier2_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["data"]["key"], session_2_key);

    // 10. Close session 2
    let (status, close_res) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_2_key}/close"),
        Some(json!({
            "countedCashCents": 5000,
            "closingFloatCents": 5000,
            "cashDropToSafeCents": 0
        })),
        &cashier2_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {close_res}");

    // 11. Now switching back to isolated mode succeeds
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
