// Editing the payee directory: rename, phone, hide and show landlords and companies, and what
// hiding does to the pickers and to records that already point at a payee, on SQLite (the
// desktop) and on a throwaway Atlas database (the cloud).

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

fn names(body: &Value) -> Vec<String> {
    body["data"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap().to_string()).collect()
}

async fn scenario(app: &common::TestApp) {
    let staff = common::mint_token(&app.config, Some(Role::Staff), roles::default_permissions(Role::Staff));
    let manager = common::mint_token(&app.config, Some(Role::Manager), roles::default_permissions(Role::Manager));
    let rent_cat = category_key(CashMovementType::RentAndBills, "Shop rent");

    let add = |name: &str| json!({ "payeeType": "vendor", "name": name });
    let (status, body) = send(app, "POST", "/api/payees", Some(add("Mr. Landlord")), &manager).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["active"], true);
    let landlord = body["data"]["key"].as_str().unwrap().to_string();
    let (status, body) = send(app, "POST", "/api/payees", Some(add("Water Board")), &manager).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // Only someone allowed to manage payees can change one; an unknown key is a 404.
    let (status, _) = send(app, "PATCH", &format!("/api/payees/{landlord}"), Some(json!({ "name": "X Y" })), &staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = send(app, "PATCH", "/api/payees/pye_vendor_missing", Some(json!({ "name": "Nobody" })), &manager).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "PAYEE_NOT_FOUND");

    // Suppliers and staff are changed on their own pages.
    let (status, _) = send(app, "PATCH", "/api/payees/sup_anything", Some(json!({ "name": "Nobody" })), &manager).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(app, "PATCH", "/api/payees/emp_anything", Some(json!({ "name": "Nobody" })), &manager).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Rename keeps the key and the history; the phone is set and then cleared with a blank.
    let (status, body) = send(
        app,
        "PATCH",
        &format!("/api/payees/{landlord}"),
        Some(json!({ "name": "  Mrs.  Landlady ", "phone": "0771234567" })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["key"], landlord.as_str());
    assert_eq!(body["data"]["name"], "Mrs. Landlady");
    assert_eq!(body["data"]["phone"], "0771234567");
    let (_, body) = send(app, "PATCH", &format!("/api/payees/{landlord}"), Some(json!({ "phone": "  " })), &manager).await;
    assert!(body["data"].get("phone").is_none(), "{body}");

    // A name must be 2 to 60 letters and cannot be another payee's name.
    let (status, _) = send(app, "PATCH", &format!("/api/payees/{landlord}"), Some(json!({ "name": "A" })), &manager).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = send(app, "PATCH", &format!("/api/payees/{landlord}"), Some(json!({ "name": "water  board" })), &manager).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "PAYEE_NAME_TAKEN");

    // Adding the old name again is refused: its key still belongs to the renamed payee.
    let (status, body) = send(app, "POST", "/api/payees", Some(add("Mr. Landlord")), &manager).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "PAYEE_NAME_TAKEN");

    // A regular bill and a payment name the landlord.
    let (status, body) = send(
        app,
        "POST",
        "/api/recurring-expenses",
        Some(json!({
            "categoryKey": rent_cat, "payeeKey": landlord, "amountCents": 5_000_000,
            "dueDay": 1, "remindDaysBefore": 0, "paidFrom": "bank"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let bill = body["data"]["key"].as_str().unwrap().to_string();

    // Hiding takes the payee out of the pickers; the manager's list can still show it.
    let (status, body) = send(app, "PATCH", &format!("/api/payees/{landlord}"), Some(json!({ "active": false })), &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["active"], false);
    let (_, body) = send(app, "GET", "/api/payees?payeeType=vendor", None, &staff).await;
    assert_eq!(names(&body), vec!["Water Board"]);
    let (status, _) = send(app, "GET", "/api/payees?includeHidden=true", None, &staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, body) = send(app, "GET", "/api/payees?payeeType=vendor&includeHidden=true", None, &manager).await;
    let all = names(&body);
    assert!(all.contains(&"Mrs. Landlady".to_string()) && all.contains(&"Water Board".to_string()), "{all:?}");
    let hidden = body["data"].as_array().unwrap().iter().find(|p| p["key"] == landlord.as_str()).unwrap();
    assert_eq!(hidden["active"], false);

    // A hidden name cannot be added again, and a hidden payee cannot be used on a new record.
    let (status, body) = send(app, "POST", "/api/payees", Some(add("MRS.  landlady")), &manager).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "PAYEE_HIDDEN");
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "terminalId": "term_payees_test", "movementType": "rent_and_bills", "amountCents": 1000,
            "categoryKey": rent_cat, "payeeKey": landlord,
            "periodStart": "2026-10-01", "periodEnd": "2026-10-31"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "PAYEE_HIDDEN");

    // The regular bill keeps showing the hidden payee's name.
    let (_, body) = send(app, "GET", "/api/recurring-expenses", None, &manager).await;
    let row = body["data"].as_array().unwrap().iter().find(|b| b["template"]["key"] == bill.as_str()).unwrap();
    assert_eq!(row["template"]["payeeName"], "Mrs. Landlady");

    // Showing it again puts it back in the pickers.
    let (status, _) = send(app, "PATCH", &format!("/api/payees/{landlord}"), Some(json!({ "active": true })), &manager).await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = send(app, "GET", "/api/payees?payeeType=vendor", None, &staff).await;
    assert!(names(&body).contains(&"Mrs. Landlady".to_string()));

    // A name typed on a petty expense is saved as an `other` payee; hiding it does not stop the
    // same name being typed again, and it stays hidden.
    let tea = category_key(CashMovementType::PettyExpense, "Tea and refreshments");
    let float = category_key(CashMovementType::AddCash, "Opening float");
    let movement = |extra: Value| {
        let mut base = json!({ "terminalId": "term_payees_test", "amountCents": 1500 });
        base.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        base
    };
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(movement(json!({ "movementType": "add_cash", "amountCents": 1_000_000, "categoryKey": float }))),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let spend = || movement(json!({ "movementType": "petty_expense", "categoryKey": tea, "payeeName": "Corner shop" }));
    let (status, body) = send(app, "POST", "/api/sessions/movements", Some(spend()), &staff).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, body) = send(app, "GET", "/api/payees?payeeType=other", None, &staff).await;
    let corner = body["data"][0]["key"].as_str().unwrap().to_string();
    assert_eq!(body["data"][0]["name"], "Corner shop");
    assert_eq!(body["data"][0]["usageCount"], 1);
    let (status, _) = send(app, "PATCH", &format!("/api/payees/{corner}"), Some(json!({ "active": false })), &manager).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = send(app, "POST", "/api/sessions/movements", Some(spend()), &staff).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, body) = send(app, "GET", "/api/payees?payeeType=other", None, &staff).await;
    assert!(body["data"].as_array().unwrap().is_empty(), "{body}");
    let (_, body) = send(app, "GET", "/api/payees?payeeType=other&includeHidden=true", None, &manager).await;
    assert_eq!(body["data"][0]["active"], false);
    assert_eq!(body["data"][0]["usageCount"], 2);
}

#[tokio::test]
async fn payees_can_be_renamed_hidden_and_shown_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn payees_can_be_renamed_hidden_and_shown_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}

// A Manager registers a supplier from the cash window; it is listed as a payee at once and a
// supplier payment may name it, though the Manager still cannot read the supplier list itself.
async fn manager_registered_supplier_scenario(app: &common::TestApp) {
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
    let name = format!(
        "Kandy Traders {}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );

    let (status, body) = send(
        app,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": name,
            "contactPerson": "Nimal",
            "primaryPhone": "077 123 4567",
            "address": "1 Temple Road, Kandy",
            "suppliedCategories": ["Phone Parts"],
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let supplier_key = body["data"]["key"].as_str().unwrap().to_string();

    let (status, body) = send(app, "GET", "/api/payees?payeeType=supplier", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let listed = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["key"] == supplier_key.as_str())
        .unwrap_or_else(|| panic!("new supplier missing from payees: {body}"));
    assert_eq!(listed["name"], name.as_str());
    assert_eq!(listed["payeeType"], "supplier");

    let (status, _) = send(app, "GET", "/api/suppliers", None, &manager).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "add_cash", "amountCents": 5_000_000, "terminalId": "term_sup",
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
            "movementType": "supplier_payment", "amountCents": 30_000, "terminalId": "term_sup",
            "categoryKey": category_key(CashMovementType::SupplierPayment, "Other"),
            "payeeKey": supplier_key
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["payeeKey"], supplier_key.as_str());
    assert_eq!(body["data"]["payeeName"], name.as_str());
}

#[tokio::test]
async fn a_manager_can_register_a_supplier_and_pay_it_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    manager_registered_supplier_scenario(&app).await;
}

#[tokio::test]
async fn a_manager_can_register_a_supplier_and_pay_it_on_mongo() {
    let app = common::spawn_app_isolated().await;
    manager_registered_supplier_scenario(&app).await;
    app.db.drop().await.ok();
}
