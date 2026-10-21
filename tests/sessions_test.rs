mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use serde_json::{Value, json};
use simplebash_pos_backend::{core::constants::roles, domain::users::Role};
use tower::ServiceExt;

async fn execute(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

async fn send_authed(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    let req = if let Some(value) = body {
        builder = builder.header(CONTENT_TYPE, "application/json");
        builder.body(Body::from(value.to_string())).unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    execute(router, req).await
}

async fn send_authed_with_device(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
    device_id: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    if let Some(did) = device_id {
        builder = builder.header("x-device-id", did);
    }
    let req = if let Some(value) = body {
        builder = builder.header(CONTENT_TYPE, "application/json");
        builder.body(Body::from(value.to_string())).unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    execute(router, req).await
}

fn mint_token_for(
    config: &simplebash_pos_backend::core::config::Config,
    user_id_or_username: &str,
    role: Option<Role>,
    permissions: &[&str],
) -> String {
    let claims = simplebash_pos_backend::core::middleware::auth::Claims {
        sub: user_id_or_username.to_string(),
        exp: 9_999_999_999,
        role,
        permissions: permissions.iter().map(|p| p.to_string()).collect(),
        tid: None,
        scope: None,
        did: None,
        name: Some("Test Cashier".to_string()),
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .unwrap()
}

#[tokio::test]
async fn session_full_lifecycle_and_z_report() {
    let app = common::spawn_app().await;

    // Seed Admin
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, None, None)
        .await
        .unwrap();

    let admin_token = mint_token_for(
        &app.config,
        "admin",
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );

    let mgr_username = format!("mgr_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let staff_username = format!("stf_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    // Create Manager
    let (status, mgr_res) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Manager Bob",
            "username": mgr_username,
            "password": "Password123!",
            "role": "manager",
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Mgr body: {mgr_res}");
    let manager_id = mgr_res["data"]["id"].as_str().unwrap();

    let manager_token = mint_token_for(
        &app.config,
        manager_id,
        Some(Role::Manager),
        roles::default_permissions(Role::Manager),
    );

    // Create Staff
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
    let staff_id = staff_res["data"]["id"].as_str().unwrap();

    let staff_token = mint_token_for(
        &app.config,
        staff_id,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );

    // 1. Set PINs for Admin and Manager
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/auth/set-pin",
        Some(json!({ "pin": "9999" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/auth/set-pin",
        Some(json!({ "pin": "5555" })),
        &manager_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    // Verify PIN works
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/auth/verify-pin",
        Some(json!({ "pin": "9999", "requiredRole": "admin" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["valid"], true);

    // 2. Open session on unique terminal with float 10,000 cents (Rs 100)
    let terminal_id = format!("term_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({
            "terminalId": terminal_id,
            "openingFloatCents": 10000,
            "openingDenominations": { "note_100": 1 },
            "openingNote": "Morning Shift"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    let session_key = body["data"]["key"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["status"], "open");
    assert_eq!(body["data"]["openingFloatCents"], 10000);

    // Guard: Opening another session on same terminal should conflict
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({
            "terminalId": terminal_id,
            "openingFloatCents": 10000
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Check Drawer State
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/drawer-state/{terminal_id}"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["currentBalanceCents"], 10000);

    // 3. Record Pay-in: 5,000 cents (Rs 50 change)
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "pay_in",
            "amountCents": 5000,
            "sessionKey": session_key,
            "terminalId": terminal_id,
            "reason": "Change float top-up"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");

    // 4. Record Pay-out: 2,000 cents (tea/coffee)
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "pay_out_expense",
            "amountCents": 2000,
            "sessionKey": session_key,
            "terminalId": terminal_id,
            "reason": "Refreshments"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");

    // 5. Record Owner Withdrawal:
    // a) Over threshold (> 50,000 cents) without PIN fails
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "owner_withdrawal",
            "amountCents": 60000,
            "sessionKey": session_key,
            "terminalId": terminal_id,
            "recipientOrSource": "Mr. Silva (Owner)",
            "reason": "Personal Drawings"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // b) Over threshold with Manager PIN (5555) fails because admin is required
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "owner_withdrawal",
            "amountCents": 60000,
            "sessionKey": session_key,
            "terminalId": terminal_id,
            "recipientOrSource": "Mr. Silva (Owner)",
            "reason": "Personal Drawings",
            "managerPin": "5555"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // c) Over threshold with Admin PIN (9999) succeeds
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "owner_withdrawal",
            "amountCents": 60000,
            "sessionKey": session_key,
            "terminalId": terminal_id,
            "recipientOrSource": "Mr. Silva (Owner)",
            "reason": "Personal Drawings",
            "managerPin": "9999"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");

    // 6. Complete a Sale linked to this session
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "sessionKey": session_key,
            "staff": { "cashierName": "Alice" },
            "items": [{
                "sourceType": "retail",
                "name": "Quick Item",
                "unitPriceCents": 80000,
                "quantity": 1,
                "discountCents": 0
            }],
            "payment": { "paymentMethod": "cash", "isCredit": false },
            "shopProfileSnapshot": { "tradingName": "Test Shop" }
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["invoice"]["sessionKey"], session_key);

    // Verify session sales and drawer balance:
    // opening: 10,000 + pay_in: 5,000 - pay_out: 2,000 - owner: 60,000 + sale: 80,000 = 33,000 cents
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/{session_key}"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["totalCashSalesCents"], 80000);
    assert_eq!(body["data"]["totalPayInCents"], 5000);
    assert_eq!(body["data"]["totalPayOutCents"], 2000);
    assert_eq!(body["data"]["totalOwnerWithdrawalsCents"], 60000);

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/drawer-state/{terminal_id}"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["currentBalanceCents"], 33000);

    // 6b. Mid-session Z-Report & document preview (while open)
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/{session_key}/z-report"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["totalInvoicesCount"], 1);
    assert_eq!(body["data"]["discountCents"], 0);
    let cash_pm = body["data"]["paymentMethods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pm| pm["method"] == "cash")
        .expect("cash tender must be present");
    assert_eq!(cash_pm["count"], 1);
    assert_eq!(cash_pm["totalCents"], 80000);

    // Mid-session PDF document fetch
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/sessions/{session_key}/document?design=current"
        ))
        .header(AUTHORIZATION, format!("Bearer {staff_token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let mid_pdf_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(
        String::from_utf8_lossy(&mid_pdf_bytes).contains("tpl_z_report")
            || mid_pdf_bytes.starts_with(b"%PDF-")
    );

    // Fetch again without data change: should serve cached PDF
    let req2 = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/sessions/{session_key}/document?design=current"
        ))
        .header(AUTHORIZATION, format!("Bearer {staff_token}"))
        .body(Body::empty())
        .unwrap();
    let resp2 = app.router.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let mid_pdf_bytes2 = axum::body::to_bytes(resp2.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(mid_pdf_bytes, mid_pdf_bytes2);

    // 7. Submit Blind Count: 33,000 cents
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/blind-count"),
        Some(json!({
            "countedCashCents": 33000,
            "closingDenominations": { "note_1000": 33 }
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["status"], "closing");

    // 8. Close session: closing float 10,000, safe deposit 23,000 (total = 33,000)
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/close"),
        Some(json!({
            "countedCashCents": 33000,
            "closingFloatCents": 10000,
            "cashDropToSafeCents": 23000,
            "closingNote": "Shift reconciled perfectly"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["status"], "closed");
    assert_eq!(body["data"]["cashVarianceCents"], 0);

    // Drawer state now has closing float 10,000 for next shift
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/drawer-state/{terminal_id}"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["currentBalanceCents"], 10000);
    assert_eq!(body["data"]["lastClosingFloatCents"], 10000);

    // 9. Generate Z-Report
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/{session_key}/z-report"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["grossSalesCents"], 80000);
    assert_eq!(body["data"]["netSalesCents"], 80000);
    assert_eq!(body["data"]["totalInvoicesCount"], 1);
    assert_eq!(body["data"]["discountCents"], 0);
    let cash_pm = body["data"]["paymentMethods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pm| pm["method"] == "cash")
        .expect("cash tender must be present");
    assert_eq!(cash_pm["count"], 1);
    assert_eq!(cash_pm["totalCents"], 80000);
    assert_eq!(
        body["data"]["cashReconciliation"]["expectedCashCents"],
        33000
    );
    assert_eq!(
        body["data"]["cashReconciliation"]["countedCashCents"],
        33000
    );
    assert_eq!(body["data"]["cashReconciliation"]["varianceCents"], 0);
    assert_eq!(
        body["data"]["cashReconciliation"]["closingFloatCents"],
        10000
    );
    assert_eq!(
        body["data"]["cashReconciliation"]["safeDepositCents"],
        23000
    );

    // 9b. Verify Z-Report document design status
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/{session_key}/document/design"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["current"]["title"], "Z-Report");

    // 9c. Fetch Z-Report PDF document
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/sessions/{session_key}/document?design=current"
        ))
        .header(AUTHORIZATION, format!("Bearer {staff_token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get(CONTENT_TYPE).unwrap(), "application/pdf");
    let pdf_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(
        String::from_utf8_lossy(&pdf_bytes).contains("tpl_z_report")
            || pdf_bytes.starts_with(b"%PDF-")
    );

    // 10. Reopen session by Admin
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/reopen"),
        Some(json!({ "reason": "Manager audit recount" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["status"], "reopened");
}

#[tokio::test]
async fn session_discrepancy_outside_movement_and_variance_conversion() {
    let app = common::spawn_app().await;

    // Seed Admin
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, None, None)
        .await
        .unwrap();

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
    let staff_id = staff_res["data"]["id"].as_str().unwrap();

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
    let manager_id = mgr_res["data"]["id"].as_str().unwrap();

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
    let session1_key = body["data"]["key"].as_str().unwrap().to_string();

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
            "recipientOrSource": "Owner Outside Shift",
            "reason": "Emergency petty cash",
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
        &format!("/api/sessions/drawer-state/{terminal_id}"),
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
    let session2_key = body["data"]["key"].as_str().unwrap().to_string();
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
async fn test_shared_vs_isolated_register_shift_mode_and_mid_shift_guard() {
    let app = common::spawn_app_isolated().await;

    // Seed Admin
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, None, None)
        .await
        .unwrap();

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
    let session_1_key = open_res["data"]["key"].as_str().unwrap().to_string();

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
    let session_2_key = open_res["data"]["key"].as_str().unwrap().to_string();

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
    assert_eq!(
        res["data"]["key"], session_2_key,
        "Expected fallback to cashier active session in shared mode"
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
async fn test_shared_vs_isolated_register_shift_mode_and_mid_shift_guard_sqlite() {
    let app = common::spawn_app_sqlite().await;

    // Seed Admin
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, None, None)
        .await
        .unwrap();

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
    let session_1_key = open_res["data"]["key"].as_str().unwrap().to_string();

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
    let session_2_key = open_res["data"]["key"].as_str().unwrap().to_string();

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
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, None, None)
        .await
        .unwrap();

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
    let staff_id = staff_res["data"]["id"].as_str().unwrap();

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
    let session_key = open_res["data"]["key"].as_str().unwrap();

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

    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, None, None)
        .await
        .unwrap();

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
    let cashier_id = users_res["data"]["id"].as_str().unwrap();

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
    .unwrap();

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
