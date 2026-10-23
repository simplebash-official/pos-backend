// Drawer cash categories, the payee directory and the typed cash movement rules, on
// SQLite (the desktop) and on a throwaway Atlas database (the cloud).

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    core::constants::roles,
    domain::{cash_categories::category_key, sessions::CashMovementType, users::Role},
};
use tower::ServiceExt;

async fn send(app: &common::TestApp, method: &str, uri: &str, body: Option<Value>, token: &str) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    let request = match body {
        Some(value) => {
            builder = builder.header(CONTENT_TYPE, "application/json");
            builder.body(Body::from(value.to_string())).unwrap()
        }
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() };
    (status, json)
}

fn key(t: CashMovementType, name: &str) -> String {
    category_key(t, name)
}

struct Tokens {
    staff: String,
    manager: String,
}

fn tokens(app: &common::TestApp) -> Tokens {
    Tokens {
        staff: common::mint_token(&app.config, Some(Role::Staff), roles::default_permissions(Role::Staff)),
        manager: common::mint_token(&app.config, Some(Role::Manager), roles::default_permissions(Role::Manager)),
    }
}

async fn scenario(app: &common::TestApp) {
    let t = tokens(app);

    // The built-in list is there, best first, and filters by kind of payment.
    let (status, body) = send(app, "GET", "/api/cash-categories?movementType=petty_expense", None, &t.staff).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let petty = body["data"].as_array().unwrap();
    assert_eq!(petty.len(), 10);
    assert_eq!(petty[0]["name"], "Tea and refreshments");
    assert!(petty.iter().all(|c| c["movementType"] == "petty_expense" && c["usageCount"] == 0));

    // A cashier can add to a low-risk type; it waits for review.
    let (status, body) = send(
        app,
        "POST",
        "/api/cash-categories",
        Some(json!({ "movementType": "petty_expense", "name": "  Generator   diesel " })),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["name"], "Generator diesel");
    assert_eq!(body["data"]["reviewStatus"], "needs_review");
    assert_eq!(body["data"]["reportGroup"], "operating_expense");
    assert_eq!(body["data"]["isSystem"], false);
    let diesel_key = body["data"]["key"].as_str().unwrap().to_string();
    assert_eq!(diesel_key, key(CashMovementType::PettyExpense, "generator diesel"));

    // The same name, in any case or spacing, is refused.
    let (status, body) = send(
        app,
        "POST",
        "/api/cash-categories",
        Some(json!({ "movementType": "petty_expense", "name": "GENERATORDIESEL" })),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "CASH_CATEGORY_NAME_TAKEN");

    // Names are 2 to 40 letters.
    for bad in ["x".to_string(), "y".repeat(41)] {
        let (status, _) = send(
            app,
            "POST",
            "/api/cash-categories",
            Some(json!({ "movementType": "petty_expense", "name": bad })),
            &t.staff,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    // A cashier cannot add for rent, nor choose a report group; a manager can do both.
    let (status, body) = send(
        app,
        "POST",
        "/api/cash-categories",
        Some(json!({ "movementType": "rent_and_bills", "name": "Generator service" })),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "CASH_CATEGORY_TYPE_RESTRICTED");
    let (status, _) = send(
        app,
        "POST",
        "/api/cash-categories",
        Some(json!({ "movementType": "petty_expense", "name": "Staff lunch", "reportGroup": "payroll" })),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = send(
        app,
        "POST",
        "/api/cash-categories",
        Some(json!({ "movementType": "rent_and_bills", "name": "Generator service", "reportGroup": "liabilities" })),
        &t.manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["reviewStatus"], "approved");
    assert_eq!(body["data"]["reportGroup"], "liabilities");

    // "Did you mean?" finds close names.
    let (status, body) = send(
        app,
        "GET",
        "/api/cash-categories/similar?movementType=petty_expense&name=Fule",
        None,
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"][0]["name"], "Fuel");

    // Managing needs the manage permission; renaming keeps the key, so history follows.
    let (status, _) = send(
        app,
        "PATCH",
        &format!("/api/cash-categories/{diesel_key}"),
        Some(json!({ "name": "Generator fuel" })),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = send(
        app,
        "PATCH",
        &format!("/api/cash-categories/{diesel_key}"),
        Some(json!({ "name": "Generator fuel", "approve": true, "requiresApprovalOverCents": 500000 })),
        &t.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["key"], diesel_key.as_str());
    assert_eq!(body["data"]["name"], "Generator fuel");
    assert_eq!(body["data"]["reviewStatus"], "approved");
    assert_eq!(body["data"]["requiresApprovalOverCents"], 500000);

    // Renaming onto an existing name is refused; "Other" cannot be renamed or hidden.
    let (status, _) = send(
        app,
        "PATCH",
        &format!("/api/cash-categories/{diesel_key}"),
        Some(json!({ "name": "Fuel" })),
        &t.manager,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let other = key(CashMovementType::PettyExpense, "Other");
    let (status, _) = send(app, "PATCH", &format!("/api/cash-categories/{other}"), Some(json!({ "active": false })), &t.manager).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Movements: the typed rules. Between-shift movements need no open shift.
    let movement = |extra: Value| {
        let mut base = json!({ "terminalId": "term_cat_test", "amountCents": 1500 });
        base.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        base
    };
    // Payouts need cash in the drawer, so put some in first.
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({
            "movementType": "add_cash", "amountCents": 10_000_000,
            "categoryKey": key(CashMovementType::AddCash, "Opening float")
        }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let tea = key(CashMovementType::PettyExpense, "Tea and refreshments");
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "petty_expense", "categoryKey": tea, "payeeName": "Corner shop", "note": "Biscuits" }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["direction"], "out");
    assert_eq!(body["data"]["categoryName"], "Tea and refreshments");
    assert_eq!(body["data"]["payeeName"], "Corner shop");
    let corner_key = body["data"]["payeeKey"].as_str().unwrap().to_string();
    assert!(corner_key.starts_with("pye_other_"));

    // The typed payee is saved for next time, and frequent ones come first.
    let (status, body) = send(app, "GET", "/api/payees?search=corner", None, &t.staff).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"][0]["key"], corner_key.as_str());
    assert_eq!(body["data"][0]["usageCount"], 1);

    // Category ranking uses real usage.
    let (_, body) = send(app, "GET", "/api/cash-categories?movementType=petty_expense", None, &t.staff).await;
    assert_eq!(body["data"][0]["name"], "Tea and refreshments");
    assert_eq!(body["data"][0]["usageCount"], 1);

    // Wrong kind of category, one-off label on a non-Other category, amount rules.
    let rent_cat = key(CashMovementType::RentAndBills, "Shop rent");
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "petty_expense", "categoryKey": rent_cat }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "CASH_CATEGORY_WRONG_TYPE");
    let (status, _) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "petty_expense", "categoryKey": tea, "customLabel": "Odd job" }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "petty_expense", "categoryKey": other, "customLabel": "Odd job" }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["customLabel"], "Odd job");
    let (status, _) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "petty_expense", "categoryKey": tea, "amountCents": 0 }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let no_sale_cat = key(CashMovementType::NoSale, "Give change");
    let (status, _) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "no_sale", "categoryKey": no_sale_cat }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a no-sale open with an amount");
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "no_sale", "categoryKey": no_sale_cat, "amountCents": 0 }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["direction"], "neutral");

    // Fields that do not apply are refused, not silently dropped.
    let (status, _) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "cash_drop", "categoryKey": key(CashMovementType::CashDrop, "To safe"), "payeeName": "Someone" }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Rent, staff and supplier payments need the managed permission.
    let (status, _) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "rent_and_bills", "categoryKey": rent_cat }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A vendor is added by a manager (not a cashier), then rent needs a vendor and a period.
    let (status, _) = send(app, "POST", "/api/payees", Some(json!({ "payeeType": "vendor", "name": "Mr. Landlord" })), &t.staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = send(
        app,
        "POST",
        "/api/payees",
        Some(json!({ "payeeType": "vendor", "name": "Mr. Landlord", "phone": "0771234567" })),
        &t.manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let landlord = body["data"]["key"].as_str().unwrap().to_string();
    let (status, _) = send(app, "POST", "/api/payees", Some(json!({ "payeeType": "vendor", "name": "MR.  landlord" })), &t.manager).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = send(app, "POST", "/api/payees", Some(json!({ "payeeType": "supplier", "name": "Nope" })), &t.manager).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "rent_and_bills", "categoryKey": rent_cat, "payeeKey": landlord }))),
        &t.manager,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "rent without a period");
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({
            "movementType": "rent_and_bills", "categoryKey": rent_cat, "payeeKey": landlord,
            "periodStart": "2026-10-01", "periodEnd": "2026-10-31", "amountCents": 5000000
        }))),
        &t.manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["payeeName"], "Mr. Landlord");
    assert_eq!(body["data"]["periodStart"], "2026-10-01");

    // Payouts above a category's own limit need a manager PIN; a missing or wrong PIN is refused.
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "petty_expense", "categoryKey": diesel_key, "amountCents": 600000 }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "over the category limit needs a PIN: {body}");
    let (status, _) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "petty_expense", "categoryKey": diesel_key, "amountCents": 600000, "managerPin": "0000" }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Merge a duplicate away: movements keep their key, the name follows the target.
    let (status, body) = send(
        app,
        "POST",
        "/api/cash-categories",
        Some(json!({ "movementType": "petty_expense", "name": "Teas" })),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let teas = body["data"]["key"].as_str().unwrap().to_string();
    let (status, _) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "petty_expense", "categoryKey": teas }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = send(
        app,
        "POST",
        &format!("/api/cash-categories/{teas}/merge"),
        Some(json!({ "targetKey": tea })),
        &t.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["key"], tea.as_str());
    assert_eq!(body["data"]["usageCount"], 2);
    let (_, body) = send(app, "GET", "/api/cash-categories?movementType=petty_expense", None, &t.staff).await;
    assert!(body["data"].as_array().unwrap().iter().all(|c| c["key"] != teas.as_str()));
    let (_, body) = send(app, "GET", &format!("/api/sessions/movements?categoryKey={tea}"), None, &t.manager).await;
    let merged_rows: Vec<&Value> = body["data"]["movements"].as_array().unwrap().iter().filter(|m| m["categoryKey"] == teas.as_str()).collect();
    assert_eq!(merged_rows.len(), 1, "{body}");
    assert_eq!(merged_rows[0]["categoryName"], "Tea and refreshments");
    // A merged category can no longer be used, and built-in ones cannot be merged away.
    let (status, _) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "petty_expense", "categoryKey": teas }))),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = send(
        app,
        "POST",
        &format!("/api/cash-categories/{tea}/merge"),
        Some(json!({ "targetKey": key(CashMovementType::PettyExpense, "Fuel") })),
        &t.manager,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Hidden categories leave the picker but stay for the management screen.
    let fuel = key(CashMovementType::PettyExpense, "Fuel");
    let (status, _) = send(app, "PATCH", &format!("/api/cash-categories/{fuel}"), Some(json!({ "active": false })), &t.manager).await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = send(app, "GET", "/api/cash-categories?movementType=petty_expense", None, &t.staff).await;
    assert!(body["data"].as_array().unwrap().iter().all(|c| c["name"] != "Fuel"));
    let (status, _) = send(app, "GET", "/api/cash-categories?includeHidden=true", None, &t.staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, body) = send(app, "GET", "/api/cash-categories?movementType=petty_expense&includeHidden=true", None, &t.manager).await;
    assert!(body["data"].as_array().unwrap().iter().any(|c| c["name"] == "Fuel" && c["active"] == false));
    let (status, body) = send(
        app,
        "POST",
        "/api/cash-categories",
        Some(json!({ "movementType": "petty_expense", "name": "fuel" })),
        &t.staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "CASH_CATEGORY_HIDDEN");

    // Review queue.
    let (_, body) = send(app, "GET", "/api/cash-categories?needsReview=true", None, &t.manager).await;
    assert!(body["data"].as_array().unwrap().iter().all(|c| c["reviewStatus"] == "needs_review"));
}

#[tokio::test]
async fn cash_categories_and_typed_movements_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn cash_categories_and_typed_movements_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}

#[tokio::test]
async fn seeding_the_built_in_categories_twice_changes_nothing() {
    let app = common::spawn_app_sqlite().await;
    let again = simplebash_pos_backend::modules::cash_categories::service::ensure_system_categories(&app.db_handle)
        .await
        .unwrap();
    assert_eq!(again.created, 0);
    assert_eq!(again.existing, simplebash_pos_backend::seeds::cash_categories::SYSTEM_CATEGORIES.len());
}

/// A desktop database from before typed movements: the old six-type table with free-text reasons.
#[tokio::test]
async fn old_cash_movements_are_migrated_to_typed_movements_once() {
    use sqlx::{Executor, Row, sqlite::SqlitePoolOptions};

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute(
        "CREATE TABLE cash_movements (
            key TEXT PRIMARY KEY, id TEXT NOT NULL, movement_number TEXT NOT NULL UNIQUE,
            movement_type TEXT NOT NULL, amount_cents INTEGER NOT NULL, session_key TEXT,
            terminal_id TEXT NOT NULL, reason TEXT NOT NULL, recipient_or_source TEXT, supplier_key TEXT,
            recorded_by_user_id TEXT NOT NULL, recorded_by_name_snapshot TEXT NOT NULL,
            approved_by_user_id TEXT, approved_by_name_snapshot TEXT,
            recorded_late INTEGER NOT NULL DEFAULT 0, original_variance_session_key TEXT,
            version INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            deleted_at TEXT, updated_by_device TEXT
        )",
    )
    .await
    .unwrap();
    for (n, kind, amount, reason, who, supplier) in [
        (1, "pay_in", 5000, "Change float", Some("Owner"), None),
        (2, "pay_out_expense", 200, "Tea", Some("Corner shop"), None),
        (3, "pay_out_supplier", 9000, "Cables", Some("ABC Traders"), Some("sup_abc")),
        (4, "owner_withdrawal", 1000, "Drawings", Some("Mr. Silva"), None),
        (5, "cash_drop", 3000, "Excess", None, None),
        (6, "drawer_open_no_sale", 0, "Making change", None, None),
    ] {
        sqlx::query(
            "INSERT INTO cash_movements (key, id, movement_number, movement_type, amount_cents, terminal_id,
                reason, recipient_or_source, supplier_key, recorded_by_user_id, recorded_by_name_snapshot,
                created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 't1', ?, ?, ?, 'u1', 'Cashier', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')",
        )
        .bind(format!("cshm_{n}"))
        .bind(format!("{n:024}"))
        .bind(format!("CSH-{n:06}"))
        .bind(kind)
        .bind(amount)
        .bind(reason)
        .bind(who)
        .bind(supplier)
        .execute(&pool)
        .await
        .unwrap();
    }

    simplebash_pos_backend::clients::sqlite::init_db(&pool).await.unwrap();
    // Running it again must change nothing.
    simplebash_pos_backend::clients::sqlite::init_db(&pool).await.unwrap();

    let rows = sqlx::query(
        "SELECT movement_type, direction, category_key, note, payee_name, payee_key, amount_cents
         FROM cash_movements ORDER BY movement_number",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    type Migrated = (String, String, String, Option<String>, Option<String>, Option<String>);
    let got: Vec<Migrated> = rows
        .iter()
        .map(|r| {
            (
                r.get("movement_type"),
                r.get("direction"),
                r.get("category_key"),
                r.get("note"),
                r.get("payee_name"),
                r.get("payee_key"),
            )
        })
        .collect();
    let other = |t| key(t, "Other");
    assert_eq!(
        got,
        vec![
            ("add_cash".into(), "in".into(), other(CashMovementType::AddCash), Some("Change float".into()), Some("Owner".into()), None),
            ("petty_expense".into(), "out".into(), other(CashMovementType::PettyExpense), Some("Tea".into()), Some("Corner shop".into()), None),
            ("supplier_payment".into(), "out".into(), other(CashMovementType::SupplierPayment), Some("Cables".into()), Some("ABC Traders".into()), Some("sup_abc".into())),
            ("owner_withdrawal".into(), "out".into(), other(CashMovementType::OwnerWithdrawal), Some("Drawings".into()), Some("Mr. Silva".into()), None),
            ("cash_drop".into(), "neutral".into(), key(CashMovementType::CashDrop, "To safe"), Some("Excess".into()), None, None),
            ("no_sale".into(), "neutral".into(), other(CashMovementType::NoSale), Some("Making change".into()), None, None),
        ]
    );
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('cash_movements')")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(!columns.iter().any(|c| c == "reason" || c == "recipient_or_source" || c == "supplier_key"));
}

#[tokio::test]
async fn old_cash_movements_are_migrated_on_mongo_once() {
    use mongodb::bson::{Document, doc};

    let app = common::spawn_app_isolated().await;
    let movements = app.db.collection::<Document>("cash_movements");
    movements
        .insert_one(doc! {
            "key": "cshm_old1", "movement_number": "CSH-000001", "movement_type": "pay_out_supplier",
            "amount_cents": 9000_i64, "terminal_id": "t1", "reason": "Cables",
            "recipient_or_source": "ABC Traders", "supplier_key": "sup_abc",
            "recorded_by_user_id": "u1", "recorded_by_name_snapshot": "Cashier", "recorded_late": false,
            "version": 1_i64,
        })
        .await
        .unwrap();

    let dry = simplebash_pos_backend::modules::cash_categories::migration::migrate_mongo(&app.db, false, true)
        .await
        .unwrap();
    assert_eq!(dry.movements, 1);
    assert!(movements.find_one(doc! { "reason": { "$exists": true } }).await.unwrap().is_some(), "a dry run writes nothing");

    let report = simplebash_pos_backend::modules::cash_categories::migration::migrate_mongo(&app.db, false, false)
        .await
        .unwrap();
    assert_eq!(report.movements, 1);
    let again = simplebash_pos_backend::modules::cash_categories::migration::migrate_mongo(&app.db, false, false)
        .await
        .unwrap();
    assert_eq!(again.movements, 0);

    let migrated = movements.find_one(doc! { "key": "cshm_old1" }).await.unwrap().unwrap();
    assert_eq!(migrated.get_str("movement_type").unwrap(), "supplier_payment");
    assert_eq!(migrated.get_str("direction").unwrap(), "out");
    assert_eq!(migrated.get_str("category_key").unwrap(), key(CashMovementType::SupplierPayment, "Other"));
    assert_eq!(migrated.get_str("note").unwrap(), "Cables");
    assert_eq!(migrated.get_str("payee_name").unwrap(), "ABC Traders");
    assert_eq!(migrated.get_str("payee_key").unwrap(), "sup_abc");
    assert!(migrated.get("reason").is_none() && migrated.get("supplier_key").is_none());

    // And it reads through the normal API shape with the category's name.
    let t = tokens(&app);
    let (status, body) = send(&app, "GET", "/api/sessions/movements/cshm_old1", None, &t.manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["categoryName"], "Other");
    app.db.drop().await.ok();
}
