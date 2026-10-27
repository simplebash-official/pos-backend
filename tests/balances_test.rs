// Supplier balances and staff advances: what the shop owes each supplier (opening balance plus
// purchases, less payments from the drawer and the ledger) and what each staff member still owes on
// advances (given, less recovered from pay). On SQLite and on a throwaway Atlas DB.

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

fn today() -> String {
    chrono::Utc::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

fn supplier_row<'a>(body: &'a Value, key: &str) -> &'a Value {
    body["data"]["suppliers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["supplierKey"] == key)
        .unwrap()
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
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let tag = &uuid::Uuid::new_v4().simple().to_string()[..8];

    // A supplier, a product, and 10 units bought at Rs. 500 each.
    let (_, category) = send(
        app,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": format!("Bal Cat {tag}"), "icon": "box", "color": "blue" })),
        &admin,
    )
    .await;
    let category_key_ = category["data"]["key"].as_str().unwrap().to_string();
    let (_, sub) = send(
        app,
        "POST",
        &format!("/api/inventory/categories/{category_key_}/subcategories"),
        Some(json!({ "name": "Items" })),
        &admin,
    )
    .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, supplier) = send(
        app,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": format!("Bal Supplier {tag}"), "contactPerson": "Nimal", "primaryPhone": "0771234567",
            "address": "Colombo", "suppliedCategories": [category_key_]
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{supplier}");
    let supplier_key = supplier["data"]["key"].as_str().unwrap().to_string();
    let (_, product) = send(
        app,
        "POST",
        "/api/inventory/products",
        Some(json!({ "name": format!("Item {tag}"), "categoryKey": category_key_, "subcategoryKey": subcategory_key, "minStockThreshold": 1 })),
        &admin,
    )
    .await;
    let (status, body) = send(
        app,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": supplier_key, "date": chrono::Utc::now().to_rfc3339(),
            "lines": [{ "productKey": product["data"]["key"], "quantity": 10, "lineTotalCents": 500000, "sellingPriceCents": 80_000 }]
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // A cashier sees neither list.
    for uri in ["/api/balances/suppliers", "/api/balances/staff"] {
        let (status, _) = send(app, "GET", uri, None, &staff).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }

    // Until an opening balance is set a supplier has no balance: stock may have been paid for before payments were tracked.
    let (status, body) = send(app, "GET", "/api/balances/suppliers", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let row = supplier_row(&body, &supplier_key);
    assert!(
        row.get("balanceCents").is_none() && row.get("openingBalanceCents").is_none(),
        "{row}"
    );

    let opening = |cents: i64| json!({ "openingBalanceCents": cents, "asOfDate": today() });
    for (what, uri, body, expected) in [
        (
            "unknown kind",
            "/api/balances/cheque/x/opening".to_string(),
            opening(1),
            StatusCode::BAD_REQUEST,
        ),
        (
            "unknown supplier",
            "/api/balances/supplier/sup_nope/opening".to_string(),
            opening(1),
            StatusCode::NOT_FOUND,
        ),
        (
            "future date",
            format!("/api/balances/supplier/{supplier_key}/opening"),
            json!({ "openingBalanceCents": 1, "asOfDate": "2999-01-01" }),
            StatusCode::BAD_REQUEST,
        ),
        (
            "a cashier",
            format!("/api/balances/supplier/{supplier_key}/opening"),
            opening(1),
            StatusCode::FORBIDDEN,
        ),
    ] {
        let token = if what == "a cashier" {
            &staff
        } else {
            &manager
        };
        let (status, body) = send(app, "PUT", &uri, Some(body), token).await;
        assert_eq!(status, expected, "{what}: {body}");
    }

    // The shop owed Rs. 1,000 before; stock worth Rs. 5,000 arrived today.
    let (status, body) = send(
        app,
        "PUT",
        &format!("/api/balances/supplier/{supplier_key}/opening"),
        Some(opening(100_000)),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, body) = send(app, "GET", "/api/balances/suppliers", None, &manager).await;
    let row = supplier_row(&body, &supplier_key);
    assert_eq!(row["purchasedCents"], 500_000);
    assert_eq!(row["paidCents"], 0);
    assert_eq!(row["balanceCents"], 100_000 + 500_000);

    // Paid Rs. 300 from the drawer and Rs. 200 from the bank.
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "add_cash", "amountCents": 5_000_000, "terminalId": "term_bal",
            "categoryKey": category_key(CashMovementType::AddCash, "Opening float")
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "supplier_payment", "amountCents": 30_000, "terminalId": "term_bal",
            "categoryKey": category_key(CashMovementType::SupplierPayment, "Settle credit balance"),
            "payeeKey": supplier_key
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = send(
        app,
        "POST",
        "/api/expenses",
        Some(json!({
            "expenseDate": today(), "categoryKey": category_key(CashMovementType::SupplierPayment, "Settle credit balance"),
            "payeeKey": supplier_key, "amountCents": 20_000, "paidFrom": "bank"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, body) = send(app, "GET", "/api/balances/suppliers", None, &manager).await;
    let row = supplier_row(&body, &supplier_key);
    assert_eq!(row["paidCents"], 50_000);
    assert_eq!(row["balanceCents"], 600_000 - 50_000);

    // Staff advances: Rs. 800 given from the bank to a staff member.
    let (status, employee) = send(
        app,
        "POST",
        "/api/employees",
        Some(json!({ "name": format!("Advance Tech {tag}"), "phone": "0771234567", "role": "technician", "defaultSplitType": "percentage", "defaultSplitValue": 20.0 })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{employee}");
    let employee_key = employee["data"]["key"].as_str().unwrap().to_string();
    let (status, body) = send(
        app,
        "POST",
        "/api/expenses",
        Some(json!({
            "expenseDate": today(), "categoryKey": category_key(CashMovementType::StaffPayment, "Advance"),
            "payeeKey": employee_key, "amountCents": 80_000, "paidFrom": "bank"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    // A salary is not an advance.
    let (status, body) = send(
        app,
        "POST",
        "/api/expenses",
        Some(json!({
            "expenseDate": today(), "categoryKey": category_key(CashMovementType::StaffPayment, "Salary"),
            "payeeKey": employee_key, "amountCents": 1_000_000, "paidFrom": "bank"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = send(app, "GET", "/api/balances/staff", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let me = body["data"]["staff"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["employeeKey"] == employee_key.as_str())
        .unwrap();
    assert_eq!(me["givenCents"], 80_000);
    assert_eq!(me["outstandingCents"], 80_000);

    // Part is taken back from pay, never more than is owed.
    let recover =
        |cents: i64| json!({ "amountCents": cents, "recoveredOn": today(), "note": "October pay" });
    let uri = format!("/api/balances/staff/{employee_key}/recoveries");
    let (status, _) = send(app, "POST", &uri, Some(recover(10_000)), &staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = send(app, "POST", &uri, Some(recover(0)), &manager).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = send(app, "POST", &uri, Some(recover(30_000)), &manager).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["summary"]["outstandingCents"], 50_000);
    assert_eq!(body["data"]["recoveries"].as_array().unwrap().len(), 1);
    let (status, body) = send(app, "POST", &uri, Some(recover(50_001)), &manager).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "ADVANCE_OVER_RECOVERED");

    let (status, body) = send(
        app,
        "GET",
        &format!("/api/balances/staff/{employee_key}"),
        None,
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["given"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["given"][0]["categoryName"], "Advance");
    let (status, _) = send(app, "GET", "/api/balances/staff/emp_nope", None, &manager).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Rs. 100 was already owed before the records began.
    let (status, body) = send(
        app,
        "PUT",
        &format!("/api/balances/staff/{employee_key}/opening"),
        Some(opening(10_000)),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, body) = send(
        app,
        "GET",
        &format!("/api/balances/staff/{employee_key}"),
        None,
        &manager,
    )
    .await;
    assert_eq!(
        body["data"]["summary"]["outstandingCents"],
        10_000 + 80_000 - 30_000
    );
}

#[tokio::test]
async fn balances_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn balances_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}
