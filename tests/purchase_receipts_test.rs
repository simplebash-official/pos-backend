// Supplier receipts: one invoice's worth of stock received at once. The scenario runs unchanged
// on SQLite (the desktop) and on a throwaway Atlas database (the cloud); the migration test
// rebuilds a desktop database from before receipts existed.

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

async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json)
}

async fn make_product(
    router: &axum::Router,
    admin: &str,
    category_key: &str,
    subcategory_key: &str,
    name: &str,
    serialized: bool,
) -> (String, String) {
    let (status, product) = send(
        router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": name,
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "minStockThreshold": 1,
            "isSerialized": serialized,
        })),
        admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{product}");
    (
        product["data"]["id"].as_str().unwrap().to_string(),
        product["data"]["key"].as_str().unwrap().to_string(),
    )
}

async fn scenario(app: &common::TestApp) {
    let router = &app.router;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let staff = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let tag = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();

    let (_, category) = send(
        router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": format!("Rcp Cat {tag}"), "icon": "box", "color": "blue" })),
        &admin,
    )
    .await;
    let category_key = category["data"]["key"].as_str().unwrap().to_string();
    let (_, sub) = send(
        router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Items" })),
        &admin,
    )
    .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, supplier) = send(
        router,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": format!("Rcp Supplier {tag}"), "contactPerson": "Nimal",
            "primaryPhone": "0771234567", "address": "Colombo", "suppliedCategories": [category_key],
        })),
        &admin,
    )
    .await;
    let supplier_key = supplier["data"]["key"].as_str().unwrap().to_string();
    let (case_id, case_key) = make_product(
        router,
        &admin,
        &category_key,
        &subcategory_key,
        &format!("Case {tag}"),
        false,
    )
    .await;
    let (cable_id, cable_key) = make_product(
        router,
        &admin,
        &category_key,
        &subcategory_key,
        &format!("Cable {tag}"),
        false,
    )
    .await;
    let (_, phone_key) = make_product(
        router,
        &admin,
        &category_key,
        &subcategory_key,
        &format!("Phone {tag}"),
        true,
    )
    .await;

    let receipt = |reference: Option<&str>, lines: Value, confirm: bool| {
        let mut body = json!({
            "supplierKey": supplier_key,
            "date": "2026-03-01T00:00:00Z",
            "lines": lines,
            "confirmDuplicate": confirm,
        });
        if let Some(reference) = reference {
            body["referenceNo"] = json!(reference);
        }
        body
    };
    let two_lines = json!([
        { "productKey": case_key, "quantity": 10, "unitCostCents": 500, "sellingPriceCents": 900 },
        { "productKey": cable_key, "quantity": 4, "unitCostCents": 1200, "sellingPriceCents": 2000 },
    ]);
    let invoice = format!("INV-{tag}");

    // Only an admin may receive stock.
    let (status, _) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receipt(Some(&invoice), two_lines.clone(), false)),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Two products on one invoice become one receipt with two lines and one total.
    let (status, created) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receipt(Some(&invoice), two_lines.clone(), false)),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let data = &created["data"];
    assert_eq!(data["referenceNo"], invoice);
    assert_eq!(data["totalCents"], 10 * 500 + 4 * 1200);
    assert_eq!(data["supplier"]["key"], supplier_key);
    assert!(data["receiptNumber"].as_str().unwrap().starts_with("PUR-"));
    let lines = data["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|l| l["receiptKey"] == data["key"]));
    let receipt_key = data["key"].as_str().unwrap().to_string();

    // Each line brought its own stock in.
    for (id, quantity) in [(&case_id, 10), (&cable_id, 4)] {
        let (_, product) = send(
            router,
            "GET",
            &format!("/api/inventory/products/{id}"),
            None,
            &admin,
        )
        .await;
        assert_eq!(product["data"]["stockQuantity"], quantity, "{product}");
    }

    // It reads back, and lists, newest first.
    let (status, fetched) = send(
        router,
        "GET",
        &format!("/api/purchase-receipts/{receipt_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(fetched["data"]["lines"].as_array().unwrap().len(), 2);
    let (status, listed) = send(
        router,
        "GET",
        &format!("/api/purchase-receipts?supplierKey={supplier_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(listed["data"]["pagination"]["total"], 1);
    let (status, _) = send(
        router,
        "GET",
        "/api/purchase-receipts/prc_nope",
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The same invoice number from the same supplier is a warning, until confirmed.
    let one_line = json!([{ "productKey": case_key, "quantity": 1, "unitCostCents": 500, "sellingPriceCents": 900 }]);
    let (status, duplicate) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receipt(Some(&invoice), one_line.clone(), false)),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{duplicate}");
    assert_eq!(duplicate["code"], "DUPLICATE_RECEIPT");
    assert_eq!(duplicate["details"]["receiptKey"], receipt_key);
    let (_, product) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{case_id}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(
        product["data"]["stockQuantity"], 10,
        "a refused duplicate adds no stock"
    );
    let (status, confirmed) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(receipt(Some(&invoice), one_line.clone(), true)),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{confirmed}");
    // A receipt with no invoice number is never a duplicate.
    for _ in 0..2 {
        let (status, body) = send(
            router,
            "POST",
            "/api/purchase-receipts",
            Some(receipt(None, one_line.clone(), false)),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    // Validation: refused before anything is written.
    for (what, body) in [
        ("no lines", receipt(None, json!([]), false)),
        (
            "zero quantity",
            receipt(
                None,
                json!([{ "productKey": case_key, "quantity": 0, "unitCostCents": 1, "sellingPriceCents": 2 }]),
                false,
            ),
        ),
        (
            "a product twice",
            receipt(
                None,
                json!([
                    { "productKey": case_key, "quantity": 1, "unitCostCents": 1, "sellingPriceCents": 2 },
                    { "productKey": case_key, "quantity": 1, "unitCostCents": 1, "sellingPriceCents": 2 },
                ]),
                false,
            ),
        ),
        (
            "serials on a plain product",
            receipt(
                None,
                json!([{ "productKey": case_key, "quantity": 1, "unitCostCents": 1, "sellingPriceCents": 2, "serialNumbers": ["X"] }]),
                false,
            ),
        ),
        (
            "missing serials",
            receipt(
                None,
                json!([{ "productKey": phone_key, "quantity": 2, "unitCostCents": 1, "sellingPriceCents": 2 }]),
                false,
            ),
        ),
    ] {
        let (status, response) =
            send(router, "POST", "/api/purchase-receipts", Some(body), &admin).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: {response}");
    }
    let (status, _) = send(
        router,
        "POST",
        "/api/purchase-receipts",
        Some(json!({
            "supplierKey": "sup_missing", "date": "2026-03-01T00:00:00Z", "lines": one_line,
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(router, "POST", "/api/purchase-receipts", Some(receipt(None, json!([{ "productKey": "prod_missing", "quantity": 1, "unitCostCents": 1, "sellingPriceCents": 2 }]), false)), &admin).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A failed second line takes the first line with it.
    let (_, before) = send(
        router,
        "GET",
        &format!("/api/purchase-receipts?supplierKey={supplier_key}"),
        None,
        &admin,
    )
    .await;
    let (status, _) = send(router, "POST", "/api/purchase-receipts", Some(receipt(None, json!([
        { "productKey": cable_key, "quantity": 5, "unitCostCents": 100, "sellingPriceCents": 200 },
        { "productKey": "prod_missing", "quantity": 1, "unitCostCents": 1, "sellingPriceCents": 2 },
    ]), false)), &admin).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, product) = send(
        router,
        "GET",
        &format!("/api/inventory/products/{cable_id}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(product["data"]["stockQuantity"], 4);
    let (_, after) = send(
        router,
        "GET",
        &format!("/api/purchase-receipts?supplierKey={supplier_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(
        before["data"]["pagination"]["total"],
        after["data"]["pagination"]["total"]
    );
}

/// What is paid as the stock arrives: from the drawer, from the owner's pocket, from the bank;
/// in full, in part, or not at all; and what a refused or reversed payment leaves behind.
async fn payment_scenario(app: &common::TestApp) {
    use simplebash_pos_backend::domain::{
        cash_categories::category_key, sessions::CashMovementType,
    };

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
    let category_key_ = category["data"]["key"].as_str().unwrap().to_string();
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
        .unwrap()
        .to_string();
    let (_, supplier) = send(router, "POST", "/api/suppliers", Some(json!({
        "name": format!("Pay Supplier {tag}"), "contactPerson": "Nimal", "primaryPhone": "0771234567",
        "address": "Colombo", "suppliedCategories": [category_key_],
    })), &admin).await;
    let supplier_key = supplier["data"]["key"].as_str().unwrap().to_string();
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
    let session_key = session["data"]["key"].as_str().unwrap().to_string();
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
            body["data"]["expectedCashCents"].as_i64().unwrap()
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
            body["data"]["stockQuantity"].as_i64().unwrap()
        }
    };
    // 10 units at Rs. 100 = Rs. 1,000.00 (100_000 cents) is too much for the Rs. 500 float; use 40.00 a unit.
    let receipt = |units: i64, payment: Option<Value>| {
        let mut body = json!({
            "supplierKey": supplier_key, "date": "2026-03-01T00:00:00Z",
            "lines": [{ "productKey": product_key, "quantity": units, "unitCostCents": 4_000, "sellingPriceCents": 6_000 }],
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
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
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
        .unwrap()
        .iter()
        .filter(|m| m["receiptKey"] == own["data"]["key"])
        .collect();
    let mut kinds: Vec<&str> = with_receipt
        .iter()
        .map(|m| m["movementType"].as_str().unwrap())
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, ["add_cash", "supplier_payment"]);

    // Reversing the payment undoes the pair, and the receipt is owed again.
    let payment_key = own["data"]["settlements"][0]["paymentKey"]
        .as_str()
        .unwrap()
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
            own["data"]["key"].as_str().unwrap()
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
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
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
        .unwrap()
        .iter()
        .find(|e| e["payeeKey"] == supplier_key.as_str())
        .expect("the bank payment is in the ledger");
    assert_eq!(row["amountCents"], 8_000);
    // No day, or a terminal, or a PIN: refused for a bank payment.
    for payment in [
        json!({ "source": "bank", "amountCents": 8_000, "categoryKey": cod }),
        bank(json!({ "terminalId": terminal })),
        bank(json!({ "managerPin": "1234" })),
    ] {
        let (status, body) = send(
            router,
            "POST",
            "/api/purchase-receipts",
            Some(receipt(2, Some(payment))),
            &admin,
        )
        .await;
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
        first["data"]["key"].as_str().unwrap().to_string(),
        second["data"]["key"].as_str().unwrap().to_string(),
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
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap())
        .collect();
    assert!(
        open_keys.contains(&second_key.as_str()) && !open_keys.contains(&first_key.as_str()),
        "{open_keys:?}"
    );
    assert!(
        open["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["dueCents"].as_i64().unwrap() > 0)
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
    assert_eq!(second["data"]["settlements"].as_array().unwrap().len(), 2);
    // No day, or a terminal, or a PIN: refused for a bank payment.
    for payment in [
        json!({ "source": "bank", "amountCents": 8_000, "categoryKey": cod }),
        bank(json!({ "terminalId": terminal })),
        bank(json!({ "managerPin": "1234" })),
    ] {
        let (status, body) = send(
            router,
            "POST",
            "/api/purchase-receipts",
            Some(receipt(2, Some(payment))),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
}

/// Supplier credits (short or damaged goods, price differences) and what they do to a receipt,
/// the supplier balance and the payables report.
async fn credit_scenario(app: &common::TestApp) {
    use simplebash_pos_backend::domain::{
        cash_categories::category_key, sessions::CashMovementType,
    };

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
    let category_key_ = category["data"]["key"].as_str().unwrap().to_string();
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
        .unwrap()
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
            supplier["data"]["key"].as_str().unwrap().to_string()
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
            "lines": [{ "productKey": product_key, "quantity": units, "unitCostCents": 1_000, "sellingPriceCents": 1_500 }],
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
    let bill_key = bill["data"]["key"].as_str().unwrap().to_string();
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
    let credit_key = recorded["data"]["key"].as_str().unwrap().to_string();
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
            voided["data"]["key"].as_str().unwrap()
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
    assert_eq!(listed["data"]["items"].as_array().unwrap().len(), 2);
    let original = listed["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == credit_key.as_str())
        .unwrap();
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
                .unwrap()
                .iter()
                .filter(|s| s["supplierKey"] == other_key.as_str())
                .map(|s| s["balanceCents"].as_i64().unwrap())
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
        .unwrap()
        .iter()
        .find(|s| s["supplierKey"] == other_key.as_str())
        .unwrap();
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
async fn receipts_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn receipts_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
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
    app.db.drop().await.ok();
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
    app.db.drop().await.ok();
}

/// A desktop database from before receipts (invoice number and notes on the purchase row, no
/// header table) gets receipts on startup: purchases of one supplier, invoice number and day
/// share one, a purchase without a number has its own, the keys are derived from the purchases
/// so every device derives the same ones, and a second start changes nothing.
#[tokio::test]
async fn a_sqlite_database_from_before_receipts_is_migrated() {
    use simplebash_pos_backend::clients::sqlite;
    use sqlx::Executor;

    let path = std::env::temp_dir().join(format!("pos_receipt_mig_{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlite::connect(&url).await.unwrap();

    // Recreate the old shape on top of a fresh schema.
    for statement in [
        "DROP TABLE purchases",
        "DROP TABLE purchase_receipts",
        "CREATE TABLE purchases (
            key TEXT PRIMARY KEY, id TEXT NOT NULL, supplier_key TEXT NOT NULL, product_key TEXT NOT NULL,
            quantity INTEGER NOT NULL, unit_cost_cents INTEGER NOT NULL, selling_price_cents INTEGER NOT NULL,
            date TEXT NOT NULL, reference_no TEXT, notes TEXT, version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT, updated_by_device TEXT)",
        "INSERT INTO purchases (key, id, supplier_key, product_key, quantity, unit_cost_cents, selling_price_cents, date, reference_no, notes, created_at, updated_at) VALUES
            ('pur_bbbbbbbbbbbb', '65f1a1a1a1a1a1a1a1a1a1d1', 'sup_1', 'prod_1', 2, 100, 150, '2026-01-01T00:00:00+00:00', 'INV-1', 'first', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00'),
            ('pur_aaaaaaaaaaaa', '65f1a1a1a1a1a1a1a1a1a1d2', 'sup_1', 'prod_2', 3, 200, 250, '2026-01-01T00:00:00+00:00', ' INV-1 ', NULL, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00'),
            ('pur_cccccccccccc', '65f1a1a1a1a1a1a1a1a1a1d3', 'sup_1', 'prod_1', 1, 100, 150, '2026-01-01T00:00:00+00:00', NULL, NULL, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00'),
            ('pur_dddddddddddd', '65f1a1a1a1a1a1a1a1a1a1d4', 'sup_1', 'prod_1', 1, 100, 150, '2026-01-02T00:00:00+00:00', 'INV-1', NULL, '2026-01-02T00:00:00+00:00', '2026-01-02T00:00:00+00:00')",
    ] {
        pool.execute(statement).await.unwrap();
    }

    sqlite::init_db(&pool).await.unwrap();

    let lines: Vec<(String, String)> =
        sqlx::query_as("SELECT key, receipt_key FROM purchases ORDER BY key")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        lines,
        vec![
            ("pur_aaaaaaaaaaaa".into(), "prc_leg_pur_aaaaaaaaaaaa".into()),
            ("pur_bbbbbbbbbbbb".into(), "prc_leg_pur_aaaaaaaaaaaa".into()),
            ("pur_cccccccccccc".into(), "prc_leg_pur_cccccccccccc".into()),
            ("pur_dddddddddddd".into(), "prc_leg_pur_dddddddddddd".into()),
        ]
    );
    let receipts: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT key, receipt_number, reference_no, notes FROM purchase_receipts ORDER BY key",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        receipts,
        vec![
            (
                "prc_leg_pur_aaaaaaaaaaaa".into(),
                "PUR-Laaaaaaaaaa".into(),
                Some("INV-1".into()),
                Some("first".into())
            ),
            (
                "prc_leg_pur_cccccccccccc".into(),
                "PUR-Lcccccccccc".into(),
                None,
                None
            ),
            (
                "prc_leg_pur_dddddddddddd".into(),
                "PUR-Ldddddddddd".into(),
                Some("INV-1".into()),
                None
            ),
        ]
    );
    // The capture triggers come back after the backfill dropped them.
    let triggers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = 'sync_purchase_receipts_ins'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(triggers, 1);

    // Idempotent: a second start finds nothing to migrate.
    sqlite::init_db(&pool).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM purchase_receipts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 3);
    pool.close().await;
    std::fs::remove_file(&path).ok();
}

/// The same history in Atlas gets the same receipts a desktop derives, and a second run
/// changes nothing.
#[tokio::test]
async fn a_mongo_database_from_before_receipts_is_migrated() {
    use mongodb::bson::{DateTime, Document, doc};
    use simplebash_pos_backend::{
        modules::inventory::service::stock_batch_migration::Mode,
        modules::purchases::service::receipt_migration::migrate_mongo,
    };

    let app = common::spawn_app_isolated().await;
    let purchases = app.db.collection::<Document>("purchases");
    let day = DateTime::parse_rfc3339_str("2026-01-01T00:00:00Z").unwrap();
    let next_day = DateTime::parse_rfc3339_str("2026-01-02T00:00:00Z").unwrap();
    let legacy = |key: &str, reference: Option<&str>, notes: Option<&str>, date: DateTime| {
        let mut document = doc! {
            "key": key, "supplier_key": "sup_1", "product_key": "prod_1", "quantity": 2_i64,
            "unit_cost_cents": 100_i64, "selling_price_cents": 150_i64, "date": date,
            "version": 1_i64, "created_at": date, "updated_at": date,
        };
        if let Some(reference) = reference {
            document.insert("reference_no", reference);
        }
        if let Some(notes) = notes {
            document.insert("notes", notes);
        }
        document
    };
    purchases
        .insert_many(vec![
            legacy("pur_bbbbbbbbbbbb", Some("INV-1"), Some("first"), day),
            legacy("pur_aaaaaaaaaaaa", Some(" INV-1 "), None, day),
            legacy("pur_cccccccccccc", None, None, day),
            legacy("pur_dddddddddddd", Some("INV-1"), None, next_day),
        ])
        .await
        .unwrap();

    let dry = migrate_mongo(&app.db, Mode::DryRun).await.unwrap();
    assert_eq!((dry.purchases, dry.receipts), (4, 3));
    assert_eq!(
        app.db
            .collection::<Document>("purchase_receipts")
            .count_documents(doc! {})
            .await
            .unwrap(),
        0,
        "a dry run writes nothing"
    );

    let done = migrate_mongo(&app.db, Mode::Apply).await.unwrap();
    assert_eq!((done.purchases, done.receipts), (4, 3));
    let mut keys: Vec<(String, String)> = Vec::new();
    let mut cursor = purchases
        .find(doc! {})
        .sort(doc! { "key": 1 })
        .await
        .unwrap();
    while cursor.advance().await.unwrap() {
        let document = cursor.deserialize_current().unwrap();
        assert!(
            document.get("reference_no").is_none(),
            "the invoice number moved to the receipt"
        );
        keys.push((
            document.get_str("key").unwrap().to_string(),
            document.get_str("receipt_key").unwrap().to_string(),
        ));
    }
    assert_eq!(
        keys,
        vec![
            (
                "pur_aaaaaaaaaaaa".to_string(),
                "prc_leg_pur_aaaaaaaaaaaa".to_string()
            ),
            (
                "pur_bbbbbbbbbbbb".to_string(),
                "prc_leg_pur_aaaaaaaaaaaa".to_string()
            ),
            (
                "pur_cccccccccccc".to_string(),
                "prc_leg_pur_cccccccccccc".to_string()
            ),
            (
                "pur_dddddddddddd".to_string(),
                "prc_leg_pur_dddddddddddd".to_string()
            ),
        ]
    );
    let receipt = app
        .db
        .collection::<Document>("purchase_receipts")
        .find_one(doc! { "key": "prc_leg_pur_aaaaaaaaaaaa" })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        receipt.get_str("receipt_number").unwrap(),
        "PUR-Laaaaaaaaaa"
    );
    assert_eq!(receipt.get_str("reference_no").unwrap(), "INV-1");
    assert_eq!(receipt.get_str("notes").unwrap(), "first");

    let again = migrate_mongo(&app.db, Mode::Apply).await.unwrap();
    assert_eq!((again.purchases, again.receipts), (0, 0));
    assert_eq!(
        app.db
            .collection::<Document>("purchase_receipts")
            .count_documents(doc! {})
            .await
            .unwrap(),
        3
    );
    app.db.drop().await.ok();
}
