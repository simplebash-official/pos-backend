//! Session lifecycle tests.

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::json;
use simplebash_pos_backend::core::constants::roles;

use crate::common;
use crate::helpers::{cat, mint_token_for, send_authed};
use simplebash_pos_backend::domain::sessions::CashMovementType;
use simplebash_pos_backend::domain::users::Role;
use tower::ServiceExt;

#[tokio::test]
async fn session_full_lifecycle_and_z_report() {
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
    let manager_id = mgr_res["data"]["id"]
        .as_str()
        .expect("the value is a string");

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
    let staff_id = staff_res["data"]["id"]
        .as_str()
        .expect("the value is a string");

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
    let session_key = body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let session_number = body["data"]["sessionNumber"]
        .as_str()
        .expect("the value is a string")
        .to_string();
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
            "movementType": "add_cash",
            "amountCents": 5000,
            "sessionKey": session_key,
            "terminalId": terminal_id,
            "categoryKey": cat(CashMovementType::AddCash, "Change float top-up")
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
            "movementType": "petty_expense",
            "amountCents": 2000,
            "sessionKey": session_key,
            "terminalId": terminal_id,
            "categoryKey": cat(CashMovementType::PettyExpense, "Tea and refreshments")
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");

    // 5. Record Owner Withdrawal:
    // a) Without a PIN it fails
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "owner_withdrawal",
            "amountCents": 60000,
            "sessionKey": session_key,
            "terminalId": terminal_id,
            "payeeName": "Mr. Silva (Owner)",
            "categoryKey": cat(CashMovementType::OwnerWithdrawal, "Personal use")
        })),
        &staff_token,
    )
    .await;
    // The drawer holds less than Rs. 600, so going over it is refused before anything else.
    assert_eq!(status, StatusCode::CONFLICT);

    // b) A wrong PIN is refused
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "owner_withdrawal",
            "amountCents": 60000,
            "sessionKey": session_key,
            "terminalId": terminal_id,
            "payeeName": "Mr. Silva (Owner)",
            "categoryKey": cat(CashMovementType::OwnerWithdrawal, "Personal use"),
            "overrideReason": "Owner needs cash today",
            "managerPin": "0000"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // c) A manager PIN (5555) is enough, whatever the amount
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "owner_withdrawal",
            "amountCents": 60000,
            "sessionKey": session_key,
            "terminalId": terminal_id,
            "payeeName": "Mr. Silva (Owner)",
            "categoryKey": cat(CashMovementType::OwnerWithdrawal, "Personal use"),
            "overrideReason": "Owner needs cash today",
            "managerPin": "5555"
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

    // Verify invoice listing filtered by sessionKey with pagination
    let (status, inv_list) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices?sessionKey={session_key}&page=1&limit=15"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {inv_list}");
    assert_eq!(inv_list["data"]["total"], 1);
    assert_eq!(inv_list["data"]["page"], 1);
    assert_eq!(inv_list["data"]["limit"], 15);
    assert_eq!(inv_list["data"]["totalPages"], 1);
    assert_eq!(inv_list["data"]["invoices"][0]["sessionKey"], session_key);

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
    // The shift's movements totalled by kind and category: cash added in, a payout and the owner's drawing out.
    let summary = body["data"]["movementSummary"]
        .as_array()
        .expect("the value is an array");
    let total = |direction: &str| -> i64 {
        summary
            .iter()
            .filter(|g| g["direction"] == direction)
            .map(|g| g["totalCents"].as_i64().expect("the value is an integer"))
            .sum()
    };
    assert_eq!(total("in"), 5000, "{summary:?}");
    assert_eq!(total("out"), 2000 + 60000, "{summary:?}");
    assert!(summary.iter().all(|g| {
        g["count"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 1
            && g["categoryName"].is_string()
    }));
    assert_eq!(body["data"]["totalInvoicesCount"], 1);
    assert_eq!(body["data"]["discountCents"], 0);
    let cash_pm = body["data"]["paymentMethods"]
        .as_array()
        .expect("the value is an array")
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
        .expect("the request is well formed");
    let resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("the awaited step succeeds");
    assert_eq!(resp.status(), StatusCode::OK);
    let mid_pdf_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
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
        .expect("the request is well formed");
    let resp2 = app
        .router
        .clone()
        .oneshot(req2)
        .await
        .expect("the awaited step succeeds");
    assert_eq!(resp2.status(), StatusCode::OK);
    let mid_pdf_bytes2 = axum::body::to_bytes(resp2.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
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
        .expect("the value is an array")
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
        .expect("the request is well formed");
    let resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("the awaited step succeeds");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(CONTENT_TYPE)
            .expect("the key is present"),
        "application/pdf"
    );
    let pdf_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
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
    assert!(
        body["data"]["closedAt"].is_null(),
        "closedAt must be null upon reopen"
    );

    // 10b. Verify current session endpoint resolves the reopened session for the terminal
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions/current?terminal_id={terminal_id}"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["status"], "reopened");
    assert_eq!(body["data"]["key"], session_key);

    // 10c. Re-closing flow: blind count and close again
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/blind-count"),
        Some(json!({ "countedCashCents": 33000 })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["status"], "closing");

    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/sessions/{session_key}/close"),
        Some(json!({
            "countedCashCents": 33000,
            "closingFloatCents": 10000,
            "cashDropToSafeCents": 23000,
            "closingNote": "Final recount closed"
        })),
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["status"], "closed");
    assert!(
        !body["data"]["closedAt"].is_null(),
        "closedAt must be set upon close"
    );

    // 11. Test listing sessions with search query
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sessions?search={session_number}"),
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let found = body["data"]["sessions"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["key"], session_key);

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/sessions?search=NON_EXISTENT_SESSION_NUMBER_XYZ",
        None,
        &staff_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(
        body["data"]["sessions"]
            .as_array()
            .expect("the value is an array")
            .len(),
        0
    );
}
