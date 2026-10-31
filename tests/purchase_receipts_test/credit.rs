//! Credit tests.

use axum::http::StatusCode;

use serde_json::json;
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::helpers::{make_product, send};

/// Supplier credits (short or damaged goods, price differences) and what they do to a receipt,
/// the supplier balance and the payables report.
async fn credit_scenario(app: &common::TestApp) {
    use simplebash_pos_backend::domain::cash_categories::category_key;
    use simplebash_pos_backend::domain::sessions::CashMovementType;

    let router = &app.router;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let tag = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
    let (_, category) = send(
        router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": format!("Crd Cat {tag}"), "icon": "box", "color": "blue" })),
        &admin,
    )
    .await;
    let category_key_ = category["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let (_, sub) = send(
        router,
        "POST",
        &format!("/api/inventory/categories/{category_key_}/subcategories"),
        Some(json!({ "name": "Items" })),
        &admin,
    )
    .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let new_supplier = |name: String| {
        let (admin, category_key_) = (admin.clone(), category_key_.clone());
        async move {
            let (_, supplier) = send(
                router,
                "POST",
                "/api/suppliers",
                Some(json!({
                    "name": name, "contactPerson": "Nimal", "primaryPhone": "0771234567",
                    "address": "Colombo", "suppliedCategories": [category_key_],
                })),
                &admin,
            )
            .await;
            supplier["data"]["key"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        }
    };
    let supplier_key = new_supplier(format!("Credit Supplier {tag}")).await;
    let other_key = new_supplier(format!("Other Supplier {tag}")).await;
    let (_, product_key) = make_product(
        router,
        &admin,
        &category_key_,
        &subcategory_key,
        &format!("Item {tag}"),
        false,
    )
    .await;
    let receive = |supplier: &str, units: i64| {
        json!({
            "supplierKey": supplier, "date": "2026-03-01T00:00:00Z",
            "lines": [{ "productKey": product_key, "quantity": units, "lineTotalCents": units * 1_000, "sellingPriceCents": 1_500 }],
        })
    };
    let credit = |supplier: &str, receipt: Option<&str>, amount: i64| {
        let mut body = json!({ "supplierKey": supplier, "amountCents": amount, "reason": "short_delivery", "note": "2 cases missing" });
        if let Some(receipt) = receipt {
            body["receiptKey"] = json!(receipt);
        }
        body
    };

    let (_, bill) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receive(&supplier_key, 8)),
        &admin,
    )
    .await;
    let bill_key = bill["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let reread = || {
        let (admin, bill_key) = (admin.clone(), bill_key.clone());
        async move {
            let (_, body) = send(
                router,
                "GET",
                &format!("/api/purchase-receipts/{bill_key}"),
                None,
                &admin,
            )
            .await;
            body["data"].clone()
        }
    };

    // Only an admin records credits, for something that exists and is owed.
    let staff = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, _) = send(
        router,
        "POST",
        "/api/supplier-credits",
        Some(credit(&supplier_key, Some(&bill_key), 2_000)),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    for (what, body) in [
        ("nothing", credit(&supplier_key, Some(&bill_key), 0)),
        (
            "more than owed",
            credit(&supplier_key, Some(&bill_key), 8_001),
        ),
        (
            "another supplier's receipt",
            credit(&other_key, Some(&bill_key), 1_000),
        ),
        (
            "no such receipt",
            credit(&supplier_key, Some("prc_nope"), 1_000),
        ),
        ("no such supplier", credit("sup_nope", None, 1_000)),
    ] {
        let (status, response) =
            send(router, "POST", "/api/supplier-credits", Some(body), &admin).await;
        assert!(!status.is_success(), "{what}: {response}");
    }

    // Two cases short: Rs. 20.00 comes off the bill.
    let (status, recorded) = send(
        router,
        "POST",
        "/api/supplier-credits",
        Some(credit(&supplier_key, Some(&bill_key), 2_000)),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{recorded}");
    let credit_key = recorded["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let bill = reread().await;
    assert_eq!(
        (bill["creditedCents"].as_i64(), bill["dueCents"].as_i64()),
        (Some(2_000), Some(6_000))
    );
    assert_eq!(bill["status"], "partly_paid");
    // A second credit cannot take more than what is left.
    let (status, _) = send(
        router,
        "POST",
        "/api/supplier-credits",
        Some(credit(&supplier_key, Some(&bill_key), 6_001)),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Voiding it puts the amount back; voiding twice is refused.
    let (status, voided) = send(
        router,
        "POST",
        &format!("/api/supplier-credits/{credit_key}/void"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{voided}");
    assert_eq!(voided["data"]["reversesKey"], credit_key);
    let bill = reread().await;
    assert_eq!(
        (
            bill["creditedCents"].as_i64(),
            bill["dueCents"].as_i64(),
            bill["status"].as_str()
        ),
        (Some(0), Some(8_000), Some("unpaid"))
    );
    let (status, again) = send(
        router,
        "POST",
        &format!("/api/supplier-credits/{credit_key}/void"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");
    assert_eq!(again["code"], "CREDIT_ALREADY_VOIDED");
    let (status, _) = send(
        router,
        "POST",
        &format!(
            "/api/supplier-credits/{}/void",
            voided["data"]["key"]
                .as_str()
                .expect("the value is a string")
        ),
        None,
        &admin,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a voiding entry cannot be voided"
    );
    let (_, listed) = send(
        router,
        "GET",
        &format!("/api/supplier-credits?supplierKey={supplier_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(
        listed["data"]["items"]
            .as_array()
            .expect("the value is an array")
            .len(),
        2
    );
    let original = listed["data"]["items"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|c| c["key"] == credit_key.as_str())
        .expect("a match exists");
    assert!(original["voidedByKey"].is_string());

    // Credit again, then pay what is left: the receipt is settled by a credit and a payment together.
    send(
        router,
        "POST",
        "/api/supplier-credits",
        Some(credit(&supplier_key, Some(&bill_key), 2_000)),
        &admin,
    )
    .await;
    let (status, session) = send(
        router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": format!("term_{tag}"), "openingFloatCents": 50_000 })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let (status, body) = send(router, "POST", "/api/sessions/movements", Some(json!({
        "movementType": "supplier_payment", "amountCents": 6_000, "sessionKey": session["data"]["key"],
        "terminalId": format!("term_{tag}"), "categoryKey": category_key(CashMovementType::SupplierPayment, "Settle credit balance"),
        "payeeKey": supplier_key, "settlements": [{ "receiptKey": bill_key, "amountCents": 6_000 }],
    })), &admin).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let bill = reread().await;
    assert_eq!(
        (
            bill["status"].as_str(),
            bill["dueCents"].as_i64(),
            bill["paidCents"].as_i64(),
            bill["creditedCents"].as_i64()
        ),
        (Some("paid"), Some(0), Some(6_000), Some(2_000))
    );

    // Payables: a supplier with no opening balance is listed by what its receipts owe.
    let (_, owing) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receive(&other_key, 10)),
        &admin,
    )
    .await;
    assert_eq!(owing["data"]["dueCents"], 10_000);
    let payables = || {
        let (admin, other_key) = (admin.clone(), other_key.clone());
        async move {
            let (_, body) =
                send(router, "GET", "/api/finance-reports/payables", None, &admin).await;
            let listed: Vec<i64> = body["data"]["suppliers"]
                .as_array()
                .expect("the value is an array")
                .iter()
                .filter(|s| s["supplierKey"] == other_key.as_str())
                .map(|s| s["balanceCents"].as_i64().expect("the value is an integer"))
                .collect();
            listed
        }
    };
    assert_eq!(payables().await, vec![10_000]);
    // Once an opening balance is set it is listed by its balance, and only once.
    let (status, body) = send(
        router,
        "PUT",
        &format!("/api/balances/supplier/{other_key}/opening"),
        Some(json!({ "openingBalanceCents": 5_000, "asOfDate": "2026-01-01" })),
        &admin,
    )
    .await;
    assert!(status.is_success(), "{body}");
    assert_eq!(
        payables().await,
        vec![15_000],
        "opening 5,000 + purchased 10,000"
    );
    // A credit on the account (no receipt) lowers the balance.
    let (status, _) = send(
        router,
        "POST",
        "/api/supplier-credits",
        Some(credit(&other_key, None, 3_000)),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(payables().await, vec![12_000]);
    let (_, balances) = send(router, "GET", "/api/balances/suppliers", None, &admin).await;
    let row = balances["data"]["suppliers"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|s| s["supplierKey"] == other_key.as_str())
        .expect("a match exists");
    assert_eq!(
        (
            row["purchasedCents"].as_i64(),
            row["paidCents"].as_i64(),
            row["creditedCents"].as_i64(),
            row["balanceCents"].as_i64()
        ),
        (Some(10_000), Some(0), Some(3_000), Some(12_000))
    );
}

#[tokio::test]
async fn credits_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    credit_scenario(&app).await;
}

#[tokio::test]
async fn credits_on_mongo() {
    let app = common::spawn_app_isolated().await;
    credit_scenario(&app).await;
    app.mongo().drop().await.ok();
}
