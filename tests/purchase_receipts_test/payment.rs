//! Payment tests.

use axum::http::StatusCode;

use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::helpers::{make_product, scenario, send};

/// What is paid as the stock arrives: from the drawer, from the owner's pocket, from the bank;
/// in full, in part, or not at all; and what a refused or reversed payment leaves behind.
async fn payment_scenario(app: &common::TestApp) {
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
        Some(json!({ "name": format!("Pay Cat {tag}"), "icon": "box", "color": "blue" })),
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
    let (_, supplier) = send(router, "POST", "/api/suppliers", Some(json!({
        "name": format!("Pay Supplier {tag}"), "contactPerson": "Nimal", "primaryPhone": "0771234567",
        "address": "Colombo", "suppliedCategories": [category_key_],
    })), &admin).await;
    let supplier_key = supplier["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let (product_id, product_key) = make_product(
        router,
        &admin,
        &category_key_,
        &subcategory_key,
        &format!("Item {tag}"),
        false,
    )
    .await;

    let terminal = format!("term_{tag}");
    let (status, session) = send(
        router,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": terminal, "openingFloatCents": 50_000 })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let session_key = session["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let cod = category_key(
        CashMovementType::SupplierPayment,
        "Stock purchase (cash on delivery)",
    );

    let expected_cash = || {
        let session_key = session_key.clone();
        let admin = admin.clone();
        async move {
            let (_, body) = send(
                router,
                "GET",
                &format!("/api/sessions/{session_key}"),
                None,
                &admin,
            )
            .await;
            body["data"]["expectedCashCents"]
                .as_i64()
                .expect("the value is an integer")
        }
    };
    let stock = || {
        let (admin, product_id) = (admin.clone(), product_id.clone());
        async move {
            let (_, body) = send(
                router,
                "GET",
                &format!("/api/inventory/products/{product_id}"),
                None,
                &admin,
            )
            .await;
            body["data"]["stockQuantity"]
                .as_i64()
                .expect("the value is an integer")
        }
    };
    // 10 units at Rs. 100 = Rs. 1,000.00 (100_000 cents) is too much for the Rs. 500 float; use 40.00 a unit.
    let receipt = |units: i64, payment: Option<Value>| {
        let mut body = json!({
            "supplierKey": supplier_key, "date": "2026-03-01T00:00:00Z",
            "lines": [{ "productKey": product_key, "quantity": units, "lineTotalCents": units * 4_000, "sellingPriceCents": 6_000 }],
        });
        if let Some(payment) = payment {
            body["payment"] = payment;
        }
        body
    };
    let drawer_payment = |amount: i64, extra: Value| {
        let mut payment = json!({
            "source": "drawer", "amountCents": amount, "categoryKey": cod,
            "terminalId": terminal, "sessionKey": session_key,
        });
        payment
            .as_object_mut()
            .expect("the value is an object")
            .extend(extra.as_object().expect("the value is an object").clone());
        payment
    };

    // On credit: nothing paid, everything due.
    let (status, credit) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receipt(2, None)),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{credit}");
    let credit = &credit["data"];
    assert_eq!(
        (
            credit["totalCents"].as_i64(),
            credit["paidCents"].as_i64(),
            credit["dueCents"].as_i64()
        ),
        (Some(8_000), Some(0), Some(8_000))
    );
    assert_eq!(credit["status"], "unpaid");
    assert_eq!(credit["settlements"], json!([]));
    assert_eq!(expected_cash().await, 50_000);

    // Paid in full from the drawer: the stock comes in and the cash goes out together.
    let (status, paid) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receipt(5, Some(drawer_payment(20_000, json!({}))))),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{paid}");
    let paid = &paid["data"];
    assert_eq!(
        (paid["paidCents"].as_i64(), paid["dueCents"].as_i64()),
        (Some(20_000), Some(0))
    );
    assert_eq!(paid["status"], "paid");
    assert_eq!(paid["settlements"][0]["paymentKind"], "cash_movement");
    assert_eq!(paid["settlements"][0]["reversed"], false);
    assert_eq!(expected_cash().await, 30_000);
    assert_eq!(stock().await, 7);
    let (_, movements) = send(
        router,
        "GET",
        &format!("/api/sessions/movements?sessionKey={session_key}&movementType=supplier_payment"),
        None,
        &admin,
    )
    .await;
    let movement = &movements["data"]["movements"][0];
    assert_eq!(movement["key"], paid["settlements"][0]["paymentKey"]);
    assert_eq!(movement["receiptKey"], paid["key"]);
    assert_eq!(movement["payeeKey"], supplier_key);

    // Part paid: the rest stays owed.
    let (status, part) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receipt(5, Some(drawer_payment(5_000, json!({}))))),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{part}");
    assert_eq!(
        (
            part["data"]["paidCents"].as_i64(),
            part["data"]["dueCents"].as_i64()
        ),
        (Some(5_000), Some(15_000))
    );
    assert_eq!(part["data"]["status"], "partly_paid");
    assert_eq!(expected_cash().await, 25_000);

    // More than the receipt comes to, or no amount at all: refused, nothing written.
    for (what, amount) in [("overpaid", 20_001), ("nothing", 0)] {
        let (status, body) = send(
            router,
            "POST",
            "/api/purchase-receipts",
            Some(receipt(5, Some(drawer_payment(amount, json!({}))))),
            &admin,
        )
        .await;
        assert!(
            status == StatusCode::CONFLICT || status == StatusCode::BAD_REQUEST,
            "{what}: {status} {body}"
        );
    }
    // More than the drawer holds needs a manager's PIN and a reason: refused, the stock stays out.
    let before = stock().await;
    let (status, body) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receipt(10, Some(drawer_payment(40_000, json!({}))))),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "DRAWER_CASH_EXCEEDED");
    assert_eq!(stock().await, before, "a refused payment adds no stock");
    assert_eq!(expected_cash().await, 25_000);
    // The cash terminal is required for the drawer; a day is refused for it.
    for payment in [
        json!({ "source": "drawer", "amountCents": 1_000, "categoryKey": cod }),
        drawer_payment(1_000, json!({ "paidOn": "2026-03-01" })),
    ] {
        let (status, body) = send(
            router,
            "POST",
            "/api/purchase-receipts",
            Some(receipt(1, Some(payment))),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }

    // The owner paid with their own money: capital in, then paid out, so the drawer is unchanged.
    let cash_before = expected_cash().await;
    let (status, own) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receipt(
            3,
            Some(json!({
                "source": "owner_pocket", "amountCents": 12_000, "categoryKey": cod,
                "terminalId": terminal, "sessionKey": session_key,
            })),
        )),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{own}");
    assert_eq!(own["data"]["status"], "paid");
    assert_eq!(
        expected_cash().await,
        cash_before,
        "capital in and payment out cancel out"
    );
    let (_, movements) = send(
        router,
        "GET",
        &format!("/api/sessions/movements?sessionKey={session_key}"),
        None,
        &admin,
    )
    .await;
    let with_receipt: Vec<&Value> = movements["data"]["movements"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .filter(|m| m["receiptKey"] == own["data"]["key"])
        .collect();
    let mut kinds: Vec<&str> = with_receipt
        .iter()
        .map(|m| m["movementType"].as_str().expect("the value is a string"))
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, ["add_cash", "supplier_payment"]);

    // Reversing the payment undoes the pair, and the receipt is owed again.
    let payment_key = own["data"]["settlements"][0]["paymentKey"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let (status, body) = send(
        router,
        "POST",
        &format!("/api/sessions/movements/{payment_key}/reverse"),
        Some(json!({ "reason": "Wrong supplier" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(expected_cash().await, cash_before, "both halves are undone");
    let (_, reread) = send(
        router,
        "GET",
        &format!(
            "/api/purchase-receipts/{}",
            own["data"]["key"].as_str().expect("the value is a string")
        ),
        None,
        &admin,
    )
    .await;
    assert_eq!(
        (
            reread["data"]["paidCents"].as_i64(),
            reread["data"]["dueCents"].as_i64()
        ),
        (Some(0), Some(12_000))
    );
    assert_eq!(reread["data"]["status"], "unpaid");
    assert_eq!(reread["data"]["settlements"][0]["reversed"], true);

    // From the bank: no register involved, and it lands in the expense ledger.
    let bank = |extra: Value| {
        let mut payment = json!({ "source": "bank", "amountCents": 8_000, "categoryKey": cod, "paidOn": "2026-03-02" });
        payment
            .as_object_mut()
            .expect("the value is an object")
            .extend(extra.as_object().expect("the value is an object").clone());
        payment
    };
    let cash_before = expected_cash().await;
    let (status, banked) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receipt(2, Some(bank(json!({}))))),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{banked}");
    assert_eq!(banked["data"]["status"], "paid");
    assert_eq!(
        banked["data"]["settlements"][0]["paymentKind"],
        "expense_payment"
    );
    assert_eq!(
        expected_cash().await,
        cash_before,
        "the bank is not the drawer"
    );
    let (_, expenses) = send(
        router,
        "GET",
        "/api/expenses?from=2026-03-02&to=2026-03-02",
        None,
        &admin,
    )
    .await;
    let row = expenses["data"]["expenses"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|e| e["payeeKey"] == supplier_key.as_str())
        .expect("the bank payment is in the ledger");
    assert_eq!(row["amountCents"], 8_000);
    // No day, or a terminal, or a PIN: refused for a bank payment.
    for (payment, approval) in [
        (
            json!({ "source": "bank", "amountCents": 8_000, "categoryKey": cod }),
            None,
        ),
        (bank(json!({ "terminalId": terminal })), None),
        (bank(json!({})), Some(json!({ "managerPin": "1234" }))),
    ] {
        let mut body = receipt(2, Some(payment));
        if let Some(approval) = approval {
            body["approval"] = approval;
        }
        let (status, body) =
            send(router, "POST", "/api/purchase-receipts", Some(body), &admin).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }

    // Paying later: one payment is counted against several receipts.
    let settle = category_key(CashMovementType::SupplierPayment, "Settle credit balance");
    let credit_receipt = |reference: &str, units: i64| {
        let mut body = receipt(units, None);
        body["referenceNo"] = json!(reference);
        body
    };
    let (_, first) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(credit_receipt(&format!("A-{tag}"), 2)),
        &admin,
    )
    .await;
    let (_, second) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(credit_receipt(&format!("B-{tag}"), 3)),
        &admin,
    )
    .await;
    let (first_key, second_key) = (
        first["data"]["key"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
        second["data"]["key"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
    );
    assert_eq!(
        (
            first["data"]["dueCents"].as_i64(),
            second["data"]["dueCents"].as_i64()
        ),
        (Some(8_000), Some(12_000))
    );

    let pay = |amount: i64, parts: Value| {
        json!({
            "movementType": "supplier_payment", "amountCents": amount, "sessionKey": session_key,
            "terminalId": terminal, "categoryKey": settle, "payeeKey": supplier_key, "settlements": parts,
        })
    };
    let cash_before = expected_cash().await;
    // Refused, and nothing recorded: more than a receipt is owed, a repeated receipt, more than
    // the payment, a receipt of nobody's, and receipts on a payment that is not to a supplier.
    for (what, body) in [
        (
            "over what is due",
            pay(
                20_000,
                json!([{ "receiptKey": first_key, "amountCents": 8_001 }]),
            ),
        ),
        (
            "a receipt twice",
            pay(
                20_000,
                json!([{ "receiptKey": first_key, "amountCents": 1_000 }, { "receiptKey": first_key, "amountCents": 1_000 }]),
            ),
        ),
        (
            "more than paid",
            pay(
                5_000,
                json!([{ "receiptKey": first_key, "amountCents": 3_000 }, { "receiptKey": second_key, "amountCents": 3_000 }]),
            ),
        ),
        (
            "no such receipt",
            pay(
                5_000,
                json!([{ "receiptKey": "prc_nope", "amountCents": 1_000 }]),
            ),
        ),
        (
            "nothing against it",
            pay(
                5_000,
                json!([{ "receiptKey": first_key, "amountCents": 0 }]),
            ),
        ),
    ] {
        let (status, response) = send(
            router,
            "POST",
            "/api/sessions/movements",
            Some(body),
            &admin,
        )
        .await;
        assert!(!status.is_success(), "{what}: {response}");
    }
    assert_eq!(
        expected_cash().await,
        cash_before,
        "a refused payment moves no cash"
    );

    // 15,000 pays the first receipt in full and 7,000 of the second.
    let (status, body) = send(router, "POST", "/api/sessions/movements",
        Some(pay(15_000, json!([{ "receiptKey": first_key, "amountCents": 8_000 }, { "receiptKey": second_key, "amountCents": 7_000 }]))), &admin).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(expected_cash().await, cash_before - 15_000);
    let (_, first) = send(
        router,
        "GET",
        &format!("/api/purchase-receipts/{first_key}"),
        None,
        &admin,
    )
    .await;
    let (_, second) = send(
        router,
        "GET",
        &format!("/api/purchase-receipts/{second_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(
        (
            first["data"]["status"].as_str(),
            first["data"]["dueCents"].as_i64()
        ),
        (Some("paid"), Some(0))
    );
    assert_eq!(
        (
            second["data"]["status"].as_str(),
            second["data"]["dueCents"].as_i64()
        ),
        (Some("partly_paid"), Some(5_000))
    );
    let (_, open) = send(
        router,
        "GET",
        &format!("/api/purchase-receipts?supplierKey={supplier_key}&open=true"),
        None,
        &admin,
    )
    .await;
    let open_keys: Vec<&str> = open["data"]["items"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|r| r["key"].as_str().expect("the value is a string"))
        .collect();
    assert!(
        open_keys.contains(&second_key.as_str()) && !open_keys.contains(&first_key.as_str()),
        "{open_keys:?}"
    );
    assert!(
        open["data"]["items"]
            .as_array()
            .expect("the value is an array")
            .iter()
            .all(|r| r["dueCents"].as_i64().expect("the value is an integer") > 0)
    );

    // A payment that is not to a supplier cannot carry receipts.
    let (status, _) = send(router, "POST", "/api/sessions/movements", Some(json!({
        "movementType": "petty_expense", "amountCents": 1_000, "sessionKey": session_key, "terminalId": terminal,
        "categoryKey": category_key(CashMovementType::PettyExpense, "Tea and refreshments"),
        "settlements": [{ "receiptKey": second_key, "amountCents": 1_000 }],
    })), &admin).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // The bank can pay the rest of the second receipt.
    let (status, body) = send(router, "POST", "/api/expenses", Some(json!({
        "expenseDate": "2026-03-03", "categoryKey": settle, "payeeKey": supplier_key, "amountCents": 5_000,
        "paidFrom": "bank", "settlements": [{ "receiptKey": second_key, "amountCents": 5_000 }],
    })), &admin).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, second) = send(
        router,
        "GET",
        &format!("/api/purchase-receipts/{second_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(
        (
            second["data"]["status"].as_str(),
            second["data"]["dueCents"].as_i64()
        ),
        (Some("paid"), Some(0))
    );
    assert_eq!(
        second["data"]["settlements"]
            .as_array()
            .expect("the value is an array")
            .len(),
        2
    );
    // No day, or a terminal, or a PIN: refused for a bank payment.
    for (payment, approval) in [
        (
            json!({ "source": "bank", "amountCents": 8_000, "categoryKey": cod }),
            None,
        ),
        (bank(json!({ "terminalId": terminal })), None),
        (bank(json!({})), Some(json!({ "managerPin": "1234" }))),
    ] {
        let mut body = receipt(2, Some(payment));
        if let Some(approval) = approval {
            body["approval"] = approval;
        }
        let (status, body) =
            send(router, "POST", "/api/purchase-receipts", Some(body), &admin).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
}

#[tokio::test]
async fn receipts_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn receipts_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}

#[tokio::test]
async fn paying_with_a_receipt_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    payment_scenario(&app).await;
}

#[tokio::test]
async fn paying_with_a_receipt_on_mongo() {
    let app = common::spawn_app_isolated().await;
    payment_scenario(&app).await;
    app.mongo().drop().await.ok();
}
