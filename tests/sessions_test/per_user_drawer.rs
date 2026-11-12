//! The drawer balance belongs to the user (the shift owner), not to the machine: one user on two
//! machines shares one balance, two users never share one, and staff work from one machine.

use axum::body::Body;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::domain::sessions::CashMovementType;
use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::common::TestApp;
use crate::helpers::{cat, execute, mint_token_for, send_authed, send_authed_with_device};

fn unique(prefix: &str) -> String {
    format!(
        "{prefix}_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    )
}

async fn drawer_balance(app: &TestApp, token: &str) -> Value {
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/sessions/drawer-state",
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    body["data"]["currentBalanceCents"].clone()
}

async fn open_shift(app: &TestApp, token: &str, machine: &str, float_cents: i64) -> Value {
    let (status, body) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": machine, "openingFloatCents": float_cents })),
        token,
        Some(machine),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    body["data"].clone()
}

/// Same user on machines A and C: one balance, and cash typed on C lands on it.
async fn one_user_two_machines(app: &TestApp) {
    let admin = unique("adm");
    let token = mint_token_for(
        &app.config,
        &admin,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let (machine_a, machine_c) = (unique("mac"), unique("win"));

    let shift = open_shift(app, &token, &machine_a, 485_000).await;
    let shift_key = shift["key"]
        .as_str()
        .expect("the shift has a key")
        .to_string();

    // Machine C sees the same shift.
    let (status, current) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &token,
        Some(&machine_c),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current["data"]["key"], shift_key.as_str());

    // The Oct 24 case: an expense typed on the other machine against this shift.
    let (status, body) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "petty_expense",
            "amountCents": 71_000,
            "sessionKey": shift_key,
            "terminalId": machine_c,
            "categoryKey": cat(CashMovementType::PettyExpense, "Tea and refreshments"),
        })),
        &token,
        Some(&machine_c),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    assert_eq!(drawer_balance(app, &token).await, 414_000);

    // Close from C, then the next opening on C expects what was kept, not a per-machine leftover.
    let (status, body) = send_authed_with_device(
        &app.router,
        "POST",
        &format!("/api/sessions/{shift_key}/close"),
        Some(json!({
            "countedCashCents": 414_000,
            "closingFloatCents": 202_000,
            "cashDropToSafeCents": 212_000,
        })),
        &token,
        Some(&machine_c),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(drawer_balance(app, &token).await, 202_000);

    let next = open_shift(app, &token, &machine_c, 202_000).await;
    // Counting the kept 202,000 matches what is expected, so there is no discrepancy.
    assert_eq!(next["openingDiscrepancyCents"], 0, "Body: {next}");
}

/// Two users keep separate balances, and a second shift of the same user is refused anywhere.
async fn two_users_separate(app: &TestApp) {
    let (admin, staff) = (unique("adm"), unique("stf"));
    let admin_token = mint_token_for(
        &app.config,
        &admin,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let staff_token = mint_token_for(
        &app.config,
        &staff,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );
    open_shift(app, &admin_token, &unique("a"), 10_000).await;
    open_shift(app, &staff_token, &unique("b"), 3_000).await;
    assert_eq!(drawer_balance(app, &admin_token).await, 10_000);
    assert_eq!(drawer_balance(app, &staff_token).await, 3_000);

    let (status, body) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": unique("c"), "openingFloatCents": 10_000 })),
        &admin_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {body}");
    assert_eq!(body["code"], "SESSION_ALREADY_OPEN");
}

async fn login(app: &TestApp, username: &str, device: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(CONTENT_TYPE, "application/json");
    if let Some(device) = device {
        builder = builder.header("x-device-id", device);
    }
    let request = builder
        .body(Body::from(
            json!({ "username": username, "password": "Password123!" }).to_string(),
        ))
        .expect("the request is well formed");
    execute(&app.router, request).await
}

/// Staff with an open shift cannot sign in on another machine; managers can.
async fn staff_one_machine(app: &TestApp) {
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the admin is seeded");
    let admin_token = mint_token_for(
        &app.config,
        "admin",
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let mut ids = Vec::new();
    let mut names = Vec::new();
    for role in ["staff", "manager"] {
        let username = unique(&role[..3]);
        let (status, body) = send_authed(
            &app.router,
            "POST",
            "/api/users",
            Some(json!({
                "name": format!("Test {role}"),
                "username": username,
                "password": "Password123!",
                "role": role,
            })),
            &admin_token,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "Body: {body}");
        ids.push(
            body["data"]["id"]
                .as_str()
                .expect("the user has an id")
                .to_string(),
        );
        names.push(username);
    }

    // No shift yet: staff may sign in anywhere.
    let (machine_a, machine_b) = (unique("a"), unique("b"));
    assert_eq!(
        login(app, &names[0], Some(&machine_b)).await.0,
        StatusCode::OK
    );

    let (status, body) = login(app, &names[0], Some(&machine_a)).await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let staff_token = body["data"]["token"].as_str().expect("a token").to_string();
    open_shift(app, &staff_token, &machine_a, 2_000).await;

    // Shift open on A: A still works, B and no device are refused.
    assert_eq!(
        login(app, &names[0], Some(&machine_a)).await.0,
        StatusCode::OK
    );
    let (status, body) = login(app, &names[0], Some(&machine_b)).await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {body}");
    assert_eq!(body["code"], "SESSION_OPEN_ON_OTHER_DEVICE");
    assert_eq!(login(app, &names[0], None).await.0, StatusCode::CONFLICT);

    // A token still alive on B does not show the shift there.
    let (status, current) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &staff_token,
        Some(&machine_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        current["data"].is_null(),
        "staff shift shown on B: {current}"
    );

    // A manager with an open shift signs in anywhere.
    let manager_token = mint_token_for(
        &app.config,
        &ids[1],
        Some(Role::Manager),
        roles::default_permissions(Role::Manager),
    );
    open_shift(app, &manager_token, &machine_a, 1_000).await;
    assert_eq!(
        login(app, &names[1], Some(&machine_b)).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn one_user_two_machines_share_one_balance_sqlite() {
    one_user_two_machines(&common::spawn_app_sqlite().await).await;
}

#[tokio::test]
async fn one_user_two_machines_share_one_balance_mongo() {
    one_user_two_machines(&common::spawn_app().await).await;
}

#[tokio::test]
async fn two_users_keep_separate_balances_sqlite() {
    two_users_separate(&common::spawn_app_sqlite().await).await;
}

#[tokio::test]
async fn two_users_keep_separate_balances_mongo() {
    two_users_separate(&common::spawn_app().await).await;
}

#[tokio::test]
async fn staff_sign_in_on_one_machine_while_shift_open_sqlite() {
    staff_one_machine(&common::spawn_app_sqlite().await).await;
}

#[tokio::test]
async fn staff_sign_in_on_one_machine_while_shift_open_mongo() {
    staff_one_machine(&common::spawn_app().await).await;
}

/// A desktop database from before: Mac and Windows rows of one user, one row of another user,
/// and a machine that never had a shift.
#[tokio::test]
async fn sqlite_migration_keeps_newest_row_per_user() {
    use sqlx::Row;
    use sqlx::sqlite::SqlitePoolOptions;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("an in-memory database opens");
    sqlx::raw_sql(
        r#"
        CREATE TABLE register_sessions (
            key TEXT PRIMARY KEY, terminal_id TEXT NOT NULL, cashier_id TEXT NOT NULL,
            opened_at TEXT NOT NULL
        );
        INSERT INTO register_sessions VALUES
            ('s_mac', 'mac', 'admin', '2026-10-24T09:20:00Z'),
            ('s_win', 'win', 'admin', '2026-10-27T06:41:00Z'),
            ('s_nimal', 'till', 'nimal', '2026-10-27T05:00:00Z');
        CREATE TABLE drawer_state (
            terminal_id TEXT PRIMARY KEY, current_balance_cents INTEGER NOT NULL DEFAULT 0,
            last_session_key TEXT, last_closing_float_cents INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL
        );
        INSERT INTO drawer_state VALUES
            ('mac', 414000, 's_mac', 485000, '2026-10-24T14:22:44Z'),
            ('win', 202000, 's_win', 202000, '2026-10-27T13:24:53Z'),
            ('till', 5000, NULL, 0, '2026-10-27T12:00:00Z'),
            ('lonely', 900, NULL, 0, '2026-10-20T08:00:00Z');
        "#,
    )
    .execute(&pool)
    .await
    .expect("the old shape is created");

    simplebash_pos_backend::clients::sqlite::migrate_schema(&pool)
        .await
        .expect("the migration runs");
    // A second run is a no-op.
    simplebash_pos_backend::clients::sqlite::migrate_schema(&pool)
        .await
        .expect("the migration is idempotent");

    let rows =
        sqlx::query("SELECT user_key, current_balance_cents FROM drawer_state ORDER BY user_key")
            .fetch_all(&pool)
            .await
            .expect("the new table reads");
    let balances: Vec<(String, i64)> = rows
        .iter()
        .map(|r| (r.get("user_key"), r.get("current_balance_cents")))
        .collect();
    assert_eq!(
        balances,
        [("admin".to_string(), 202_000), ("nimal".to_string(), 5_000)]
    );
}

#[tokio::test]
async fn mongo_migration_keeps_newest_row_per_user() {
    use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
    use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
    use simplebash_pos_backend::modules::sessions::migration::migrate_mongo;

    let app = common::spawn_app_isolated().await;
    let db = app.mongo();
    let at = |millis: i64| BsonDateTime::from_millis(millis);
    db.collection::<Document>("register_sessions")
        .insert_many([
            doc! { "key": "s_mac", "terminal_id": "mac", "cashier_id": "admin", "opened_at": at(1) },
            doc! { "key": "s_win", "terminal_id": "win", "cashier_id": "admin", "opened_at": at(2) },
            doc! { "key": "s_nimal", "terminal_id": "till", "cashier_id": "nimal", "opened_at": at(3) },
        ])
        .await
        .expect("shifts are seeded");
    let drawer = |terminal: &str, cents: i64, session: Option<&str>, updated: i64| {
        let mut row = doc! {
            "terminal_id": terminal,
            "current_balance_cents": cents,
            "last_closing_float_cents": cents,
            "updated_at": at(updated),
        };
        if let Some(session) = session {
            row.insert("last_session_key", session);
        }
        row
    };
    let drawers = db.collection::<Document>("drawer_state");
    drawers
        .insert_many([
            drawer("mac", 414_000, Some("s_mac"), 10),
            drawer("win", 202_000, Some("s_win"), 20),
            drawer("till", 5_000, None, 15),
            drawer("lonely", 900, None, 5),
        ])
        .await
        .expect("old drawers are seeded");

    let dry = migrate_mongo(db, Mode::DryRun)
        .await
        .expect("the dry run works");
    assert_eq!(dry.machine_rows, 4);
    assert_eq!(dry.ownerless, ["lonely"]);
    assert_eq!(drawers.count_documents(doc! {}).await.expect("count"), 4);

    let applied = migrate_mongo(db, Mode::Apply)
        .await
        .expect("the migration applies");
    let planned: Vec<(&str, i64, &str)> = applied
        .planned
        .iter()
        .map(|p| {
            (
                p.user_key.as_str(),
                p.balance_cents,
                p.from_terminal_id.as_str(),
            )
        })
        .collect();
    assert_eq!(
        planned,
        [("admin", 202_000, "win"), ("nimal", 5_000, "till")]
    );
    assert_eq!(
        drawers
            .count_documents(doc! { "terminal_id": { "$exists": true } })
            .await
            .expect("count"),
        0
    );
    let admin = drawers
        .find_one(doc! { "user_key": "admin" })
        .await
        .expect("the read works")
        .expect("admin has a balance");
    assert_eq!(
        admin.get_i64("current_balance_cents").expect("cents"),
        202_000
    );

    let again = migrate_mongo(db, Mode::Apply)
        .await
        .expect("a second run works");
    assert_eq!(again.machine_rows, 0);
    db.drop().await.expect("the throwaway database is dropped");
}

/// With `allowStaffMultiDeviceSessions = true` in Personal mode, a Staff member logs in on two
/// terminals, sees and adds cash to their own shift/personal drawer, cannot open a duplicate shift,
/// and remains isolated from another Staff member's personal shift and drawer.
async fn staff_multi_device_personal_shift(app: &TestApp) {
    let admin_token = mint_token_for(
        &app.config,
        "admin",
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let (status, enabled) = send_authed(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "allowStaffMultiDeviceSessions": true })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {enabled}");

    let mut names = Vec::new();
    for tag in ["stfa", "stfb"] {
        let username = unique(tag);
        let (status, body) = send_authed(
            &app.router,
            "POST",
            "/api/users",
            Some(json!({
                "name": format!("Staff {tag}"),
                "username": username,
                "password": "Password123!",
                "role": "staff",
            })),
            &admin_token,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "Body: {body}");
        names.push(username);
    }

    let (machine_a, machine_b) = (unique("posA"), unique("posB"));

    let login_ok = |username: &str, device: &str| {
        let router = app.router.clone();
        let body = json!({ "username": username, "password": "Password123!" }).to_string();
        let device = device.to_string();
        async move {
            let request = Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .header("x-device-id", device)
                .body(Body::from(body))
                .expect("request builds");
            let (status, resp) = execute(&router, request).await;
            assert_eq!(status, StatusCode::OK, "login failed: {resp}");
            resp["data"]["token"].as_str().expect("token").to_string()
        }
    };

    let staff_a_tok_a = login_ok(&names[0], &machine_a).await;
    let shift_a = open_shift(app, &staff_a_tok_a, &machine_a, 10_000).await;
    let shift_a_key = shift_a["key"].as_str().expect("shift key").to_string();

    // Staff A logs in on machine B too and sees the same Personal shift.
    let staff_a_tok_b = login_ok(&names[0], &machine_b).await;
    let (status, current_a_b) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &staff_a_tok_b,
        Some(&machine_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {current_a_b}");
    assert_eq!(current_a_b["data"]["key"], shift_a_key.as_str());

    // Opening a second Personal shift for Staff A on machine B is still rejected.
    let (status, dup_open) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": machine_b, "openingFloatCents": 1_000 })),
        &staff_a_tok_b,
        Some(&machine_b),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {dup_open}");
    assert_eq!(dup_open["code"], "SESSION_ALREADY_OPEN");

    // Staff A adds cash on machine B against their open shift -> updates Staff A's personal drawer.
    let (status, add_res) = send_authed_with_device(
        &app.router,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "add_cash",
            "amountCents": 3_500,
            "sessionKey": shift_a_key,
            "terminalId": machine_b,
            "categoryKey": cat(CashMovementType::AddCash, "Change float top-up"),
        })),
        &staff_a_tok_b,
        Some(&machine_b),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {add_res}");
    assert_eq!(drawer_balance(app, &staff_a_tok_a).await, 13_500);
    assert_eq!(drawer_balance(app, &staff_a_tok_b).await, 13_500);

    // Staff B has their own separate shift & personal drawer even on the same machine B.
    let staff_b_tok = login_ok(&names[1], &machine_b).await;
    let (status, current_b) = send_authed_with_device(
        &app.router,
        "GET",
        "/api/sessions/current",
        None,
        &staff_b_tok,
        Some(&machine_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {current_b}");
    assert!(current_b["data"].is_null());

    open_shift(app, &staff_b_tok, &machine_b, 2_000).await;
    assert_eq!(drawer_balance(app, &staff_b_tok).await, 2_000);
    assert_eq!(drawer_balance(app, &staff_a_tok_a).await, 13_500);
}

#[tokio::test]
async fn staff_multi_device_shares_personal_shift_sqlite() {
    staff_multi_device_personal_shift(&common::spawn_app_sqlite().await).await;
}

#[tokio::test]
async fn staff_multi_device_shares_personal_shift_mongo() {
    let app = common::spawn_app_isolated().await;
    staff_multi_device_personal_shift(&app).await;
    app.mongo().drop().await.ok();
}
