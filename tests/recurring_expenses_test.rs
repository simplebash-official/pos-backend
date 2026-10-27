// Regular bills: templates, how each stands for the month, partial payments, and the "already paid"
// guard on rent. On SQLite and on a throwaway Atlas database.

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use chrono::{Datelike, Utc};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    core::constants::roles,
    domain::{cash_categories::category_key, sessions::CashMovementType, users::Role},
};
use tower::ServiceExt;

async fn send(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
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

fn cat(name: &str) -> String {
    category_key(CashMovementType::RentAndBills, name)
}

async fn scenario(app: &common::TestApp) {
    let staff = common::mint_token(
        &app.config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );
    let manager = common::mint_token(
        &app.config,
        Some(Role::Manager),
        roles::default_permissions(Role::Manager),
    );
    let today = Utc::now().date_naive();
    let month = today.format("%Y-%m").to_string();
    let first_day = format!("{month}-01");
    let last_day = {
        let next = if today.month() == 12 {
            chrono::NaiveDate::from_ymd_opt(today.year() + 1, 1, 1)
        } else {
            chrono::NaiveDate::from_ymd_opt(today.year(), today.month() + 1, 1)
        }
        .unwrap();
        next.pred_opt().unwrap().format("%Y-%m-%d").to_string()
    };

    let (status, _) = send(app, "GET", "/api/recurring-expenses", None, &staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        app,
        "POST",
        "/api/recurring-expenses",
        Some(json!({ "categoryKey": cat("Shop rent"), "amountCents": 100, "dueDay": 1, "remindDaysBefore": 0, "paidFrom": "bank" })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, vendor) = send(
        app,
        "POST",
        "/api/payees",
        Some(json!({ "payeeType": "vendor", "name": "Mr. Landlord" })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{vendor}");
    let landlord = vendor["data"]["key"].as_str().unwrap().to_string();

    // A regular bill that falls due today.
    let create = |extra: Value| {
        let mut base = json!({
            "categoryKey": cat("Shop rent"), "payeeKey": landlord, "amountCents": 5_000_000,
            "dueDay": today.day(), "remindDaysBefore": 3, "paidFrom": "drawer", "note": "Monthly rent"
        });
        base.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        base
    };
    let (status, body) = send(
        app,
        "POST",
        "/api/recurring-expenses",
        Some(create(json!({}))),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let rent_key = body["data"]["key"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["categoryName"], "Shop rent");
    assert_eq!(body["data"]["payeeName"], "Mr. Landlord");
    assert_eq!(body["data"]["active"], true);

    // One regular bill per category and payee; bad schedules are refused.
    let (status, body) = send(
        app,
        "POST",
        "/api/recurring-expenses",
        Some(create(json!({}))),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "RECURRING_EXPENSE_EXISTS");
    for (what, extra) in [
        (
            "amount",
            json!({ "categoryKey": cat("Water"), "amountCents": 0 }),
        ),
        (
            "due day 0",
            json!({ "categoryKey": cat("Water"), "dueDay": 0 }),
        ),
        (
            "due day 32",
            json!({ "categoryKey": cat("Water"), "dueDay": 32 }),
        ),
        (
            "reminder too early",
            json!({ "categoryKey": cat("Water"), "remindDaysBefore": 31 }),
        ),
        (
            "unpaid is not an account",
            json!({ "categoryKey": cat("Water"), "paidFrom": "unpaid" }),
        ),
        (
            "not a cost",
            json!({ "categoryKey": category_key(CashMovementType::OwnerWithdrawal, "Personal use") }),
        ),
        (
            "drawer rent without a payee",
            json!({ "categoryKey": cat("Water"), "payeeKey": null, "paidFrom": "drawer" }),
        ),
        (
            "unknown payee",
            json!({ "categoryKey": cat("Water"), "payeeKey": "pye_vendor_nope" }),
        ),
    ] {
        let (status, body) = send(
            app,
            "POST",
            "/api/recurring-expenses",
            Some(create(extra)),
            &manager,
        )
        .await;
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND,
            "{what}: {status} {body}"
        );
    }

    // Due today: inside the reminder window, nothing paid yet.
    let bill = |bills: &Value, key: &str| {
        bills["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["template"]["key"] == key)
            .cloned()
            .unwrap()
    };
    let (_, bills) = send(app, "GET", "/api/recurring-expenses", None, &manager).await;
    let rent = bill(&bills, &rent_key);
    assert_eq!(rent["period"], month.as_str());
    assert_eq!(rent["status"], "due");
    assert_eq!(rent["daysUntilDue"], 0);
    assert_eq!(rent["paidCents"], 0);
    assert_eq!(rent["balanceCents"], 5_000_000);

    // A bill with a due day before today is overdue, and one far ahead is merely upcoming.
    let mut extra_keys = Vec::new();
    if today.day() > 1 {
        let (status, body) = send(
            app, "POST", "/api/recurring-expenses",
            Some(create(json!({ "categoryKey": cat("Electricity"), "payeeKey": null, "dueDay": 1, "paidFrom": "bank" }))), &manager,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        extra_keys.push((body["data"]["key"].as_str().unwrap().to_string(), "overdue"));
    }
    if today.day() + 10 <= 28 {
        let (status, body) = send(
            app, "POST", "/api/recurring-expenses",
            Some(create(json!({ "categoryKey": cat("Garbage and security"), "payeeKey": null, "dueDay": today.day() + 10, "remindDaysBefore": 2, "paidFrom": "bank" }))), &manager,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        extra_keys.push((
            body["data"]["key"].as_str().unwrap().to_string(),
            "upcoming",
        ));
    }
    let (_, bills) = send(app, "GET", "/api/recurring-expenses", None, &manager).await;
    for (key, expected) in &extra_keys {
        assert_eq!(bill(&bills, key)["status"], *expected);
    }
    let days: Vec<i64> = bills["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["daysUntilDue"].as_i64().unwrap())
        .collect();
    assert!(
        days.windows(2).all(|w| w[0] <= w[1]),
        "most urgent first: {days:?}"
    );

    // Cash in the drawer, then rent paid in two parts against the same month.
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(
            json!({ "movementType": "add_cash", "amountCents": 50_000_000, "terminalId": "term_rec",
                     "categoryKey": category_key(CashMovementType::AddCash, "Opening float") }),
        ),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let pay = |amount: i64, extra: Value| {
        let mut base = json!({
            "movementType": "rent_and_bills", "amountCents": amount, "terminalId": "term_rec",
            "categoryKey": cat("Shop rent"), "payeeKey": landlord, "periodStart": first_day, "periodEnd": last_day
        });
        base.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        base
    };
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(pay(3_000_000, json!({}))),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let first_payment = body["data"]["key"].as_str().unwrap().to_string();

    let (_, bills) = send(app, "GET", "/api/recurring-expenses", None, &manager).await;
    let rent = bill(&bills, &rent_key);
    assert_eq!(rent["paidCents"], 3_000_000);
    assert_eq!(rent["balanceCents"], 2_000_000);
    assert_ne!(rent["status"], "paid", "30,000 of 50,000 is not paid");
    assert_eq!(rent["payments"].as_array().unwrap().len(), 1);

    // The "already paid" check says what is recorded; a part payment is not yet a duplicate.
    let status_uri = format!(
        "/api/recurring-expenses/period-status?categoryKey={}&payeeKey={landlord}&period={month}",
        cat("Shop rent")
    );
    let (status, body) = send(app, "GET", &status_uri, None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["paidCents"], 3_000_000);
    assert_eq!(body["data"]["expectedCents"], 5_000_000);
    assert_eq!(body["data"]["alreadyPaid"], false);
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(pay(2_000_000, json!({}))),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "the rest of the rent: {body}");

    let (_, bills) = send(app, "GET", "/api/recurring-expenses", None, &manager).await;
    let rent = bill(&bills, &rent_key);
    assert_eq!(rent["status"], "paid");
    assert_eq!(rent["balanceCents"], 0);

    // Paid in full: another rent payment for the month is asked about, not silently doubled.
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(pay(100_000, json!({}))),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "PERIOD_ALREADY_PAID");
    assert!(
        body["message"].as_str().unwrap().contains("Rs. 50,000.00"),
        "{body}"
    );
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(pay(100_000, json!({ "confirmDuplicate": true }))),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, body) = send(app, "GET", &status_uri, None, &manager).await;
    assert_eq!(body["data"]["alreadyPaid"], true);

    // A reversed payment stops counting.
    let (status, _) = send(
        app,
        "POST",
        &format!("/api/sessions/movements/{first_payment}/reverse"),
        Some(json!({ "reason": "Entered twice" })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, body) = send(app, "GET", &status_uri, None, &manager).await;
    assert_eq!(body["data"]["paidCents"], 2_000_000 + 100_000);

    // A bank payment for the same payee and category counts towards the month too.
    let (status, body) = send(
        app, "POST", "/api/expenses",
        Some(json!({ "expenseDate": today.format("%Y-%m-%d").to_string(), "categoryKey": cat("Shop rent"),
                     "payeeKey": landlord, "amountCents": 2_900_000, "paidFrom": "bank" })), &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, bills) = send(app, "GET", "/api/recurring-expenses", None, &manager).await;
    assert_eq!(bill(&bills, &rent_key)["paidCents"], 5_000_000);
    assert_eq!(bill(&bills, &rent_key)["status"], "paid");

    // Changing and stopping a regular bill.
    let (status, body) = send(
        app, "PATCH", &format!("/api/recurring-expenses/{rent_key}"),
        Some(json!({ "amountCents": 5_500_000, "dueDay": 7, "remindDaysBefore": 5, "paidFrom": "bank" })), &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["amountCents"], 5_500_000);
    assert_eq!(body["data"]["dueDay"], 7);
    assert_eq!(body["data"]["paidFrom"], "bank");
    let (status, _) = send(
        app,
        "PATCH",
        &format!("/api/recurring-expenses/{rent_key}"),
        Some(json!({ "dueDay": 40 })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    if let Some((bank_only, _)) = extra_keys.first() {
        let (status, body) = send(
            app,
            "PATCH",
            &format!("/api/recurring-expenses/{bank_only}"),
            Some(json!({ "paidFrom": "drawer" })),
            &manager,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "a payee-less rent bill cannot move to the drawer: {body}"
        );
    }
    let (status, _) = send(
        app,
        "PATCH",
        "/api/recurring-expenses/rexp_nope",
        Some(json!({ "active": false })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        app,
        "PATCH",
        &format!("/api/recurring-expenses/{rent_key}"),
        Some(json!({ "active": false })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, bills) = send(app, "GET", "/api/recurring-expenses", None, &manager).await;
    assert!(
        bills["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["template"]["key"] != rent_key.as_str())
    );
    let (_, bills) = send(
        app,
        "GET",
        "/api/recurring-expenses?includeInactive=true",
        None,
        &manager,
    )
    .await;
    assert!(
        bills["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["template"]["key"] == rent_key.as_str())
    );
    let (_, body) = send(app, "GET", &status_uri, None, &manager).await;
    assert!(
        body["data"].get("expectedCents").is_none(),
        "a stopped bill no longer sets an expectation"
    );
}

#[tokio::test]
async fn regular_bills_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn regular_bills_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}
