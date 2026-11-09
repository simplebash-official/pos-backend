//! Session variance tests.

use axum::http::StatusCode;

use serde_json::json;
use simplebash_pos_backend::core::constants::roles;

use simplebash_pos_backend::domain::sessions::CashMovementType;
use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::helpers::{cat, mint_token_for, send_authed, send_authed_with_device};

#[tokio::test]
async fn session_discrepancy_outside_movement_and_variance_conversion() {
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

    let staff_username = format!("stf_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let (status, staff_res) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Staff Cashier",
            "username": staff_username,
            "password": "Password123!",
            "role": "staff",
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let staff_id = staff_res["data"]["id"]
        .as_str()
        .expect("the value is a string");

    let staff_token = mint_token_for(
        &app.config,
        staff_id,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );

    let mgr_username = format!("mgr_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let (status, mgr_res) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Manager Charlie",
            "username": mgr_username,
            "password": "Password123!",
            "role": "manager",
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let manager_id = mgr_res["data"]["id"]
        .as_str()
        .expect("the value is a string");

    let manager_token = mint_token_for(
        &app.config,
        manager_id,
        Some(Role::Manager),
        roles::default_permissions(Role::Manager),
    );

    // Set Manager PIN
    let (pin_status, pin_body) = send_authed(
        &app.router,
        "POST",
        "/api/auth/set-pin",
        Some(json!({ "pin": "1234" })),
        &manager_token,
    )
    .await;
    assert_eq!(pin_status, StatusCode::OK, "Set pin failed: {pin_body}");

    let terminal_id = format!("term_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    // 1. Shift 1 opens with 5,000 float and closes leaving 5,000 in drawer
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({
            "terminalId": terminal_id,
            "openingFloatCents": 5000
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let session1_key = body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    // Close shift 1 with float 5,000
    let (status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session1_key}/close"),
        Some(json!({
            "countedCashCents": 5000,
            "closingFloatCents": 5000,
            "cashDropToSafeCents": 0
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 2. Outside-session movement: Owner takes 1,000 cents from drawer between shifts (Option A ledger)
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "owner_withdrawal",
            "amountCents": 1000,
            "terminalId": terminal_id,
            "payeeName": "Owner Outside Shift",
            "categoryKey": cat(CashMovementType::OwnerWithdrawal, "Personal use"),
            "managerPin": "1234"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");

    // Drawer state should now show 4,000 cents
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/drawer-state",
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["currentBalanceCents"], 4000);

    // 3. Shift 2 Opening: Cashier counts 3,500 cents (500 discrepancy from expected 4,000)
    // a) Opening without reason / PIN fails
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({
            "terminalId": terminal_id,
            "openingFloatCents": 3500
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // b) Opening with reason and Manager PIN succeeds
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({
            "terminalId": terminal_id,
            "openingFloatCents": 3500,
            "discrepancyReason": "Missing small change coins",
            "managerPin": "1234"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    let session2_key = body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert_eq!(body["data"]["openingDiscrepancyCents"], -500);

    // 4. Close Shift 2 with a cash shortage: expected 3,500, counted 2,500 (1,000 shortage)
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session2_key}/close"),
        Some(json!({
            "countedCashCents": 2500,
            "closingFloatCents": 2500,
            "cashDropToSafeCents": 0,
            "varianceReason": "Unknown shortage at close",
            "managerPin": "1234"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["cashVarianceCents"], -1000);

    // 5. Owner realizes they took 1,000 cents during the shift without recording it.
    // Convert the 1,000 shortage variance into a late owner withdrawal!
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session2_key}/convert-variance"),
        Some(json!({
            "reason": "Owner took money for urgent personal purchase",
            "recipient": "Shop Owner",
            "managerPin": "1234"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    assert_eq!(body["data"]["amountCents"], 1000);
    assert_eq!(body["data"]["recordedLate"], true);
    assert_eq!(body["data"]["movementType"], "owner_withdrawal");

    // Session reflects late movement
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/{session2_key}"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["hasLateMovements"], true);
}

#[tokio::test]
async fn shift_follows_its_owner_and_the_setting_guard_holds() {
    let app = common::spawn_app_isolated().await;

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

    // b) Cashier 2 on term_1 does not get Cashier 1's shift (each user sees their own)
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

    // d) Cashier 2 on unused terminal term_2 returns null
    let (status, res) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier2_token,
        Some("term_2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(res["data"].is_null());

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

    // 9. Same rules with the setting on (it no longer changes shift lookup):
    // a) Cashier 1 on term_2 does not join Cashier 2's shift
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
    assert!(res["data"].is_null(), "another user's shift leaked: {res}");

    // b) Cashier 2 gets his shift on term_2, and not without a device
    let (status, res) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier2_token,
        Some("term_2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["data"]["key"], session_2_key);
    let (status, res) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier2_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        res["data"].is_null(),
        "staff shift shown without its machine: {res}"
    );

    // c) Cashier 1 with NO device_id has no session and returns null
    let (status, res) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &cashier1_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(res["data"].is_null());

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
async fn a_signed_in_manager_approves_their_own_shift_differences() {
    let app = common::spawn_app().await;
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the admin is seeded");
    let manager_token = mint_token_for(
        &app.config,
        "usr_manager_self",
        Some(Role::Manager),
        roles::default_permissions(Role::Manager),
    );
    let terminal_id = format!("term_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    // Shift 1 closes leaving 5,000; an owner withdrawal between shifts leaves 4,000 expected.
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": terminal_id, "openingFloatCents": 5000 })),
        &manager_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let first = body["data"]["key"].as_str().expect("a key").to_string();
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{first}/close"),
        Some(json!({
            "countedCashCents": 5000, "closingFloatCents": 5000, "cashDropToSafeCents": 0
        })),
        &manager_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A counted float that differs still needs its explanation, but not a PIN; a PIN is refused.
    let opening = |extra: serde_json::Value| {
        let mut body = json!({ "terminalId": terminal_id, "openingFloatCents": 3500 });
        body.as_object_mut()
            .expect("an object")
            .extend(extra.as_object().expect("an object").clone());
        body
    };
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(opening(json!({}))),
        &manager_token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "the explanation is still required"
    );
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(opening(
            json!({ "discrepancyReason": "Coins taken", "managerPin": "1234" }),
        )),
        &manager_token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(opening(json!({ "discrepancyReason": "Coins taken" }))),
        &manager_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let second = body["data"]["key"].as_str().expect("a key").to_string();
    assert_eq!(body["data"]["openingApprovedById"], "usr_manager_self");

    // A shortage beyond the tolerance closes with the explanation and no PIN.
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{second}/close"),
        Some(json!({
            "countedCashCents": 2500, "closingFloatCents": 2500, "cashDropToSafeCents": 0,
            "varianceReason": "Unknown shortage at close"
        })),
        &manager_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["varianceApprovedById"], "usr_manager_self");

    // Converting the shortage to a late owner withdrawal needs no PIN either.
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{second}/convert-variance"),
        Some(json!({ "reason": "Owner drawings", "recipient": "Shop Owner" })),
        &manager_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["approvedByUserId"], "usr_manager_self");
}
