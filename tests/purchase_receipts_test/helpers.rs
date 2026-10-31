//! Shared setup for the `purchase_receipts_test` tests.

use crate::common;
use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::domain::users::Role;
use tower::ServiceExt;

// Supplier receipts: one invoice's worth of stock received at once. The scenario runs unchanged
// on SQLite (the desktop) and on a throwaway Atlas database (the cloud); the migration test
// rebuilds a desktop database from before receipts existed.

pub(crate) async fn send(
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
        .expect("the request is well formed");
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json)
}

pub(crate) async fn make_product(
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
        product["data"]["id"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
        product["data"]["key"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
    )
}

pub(crate) async fn scenario(app: &common::TestApp) {
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
    let category_key = category["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
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
        .expect("the value is a string")
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
    let supplier_key = supplier["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
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
        { "productKey": case_key, "quantity": 10, "lineTotalCents": 5000, "sellingPriceCents": 900 },
        { "productKey": cable_key, "quantity": 4, "lineTotalCents": 4800, "sellingPriceCents": 2000 },
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
    assert!(
        data["receiptNumber"]
            .as_str()
            .expect("the value is a string")
            .starts_with("PUR-")
    );
    let lines = data["lines"].as_array().expect("the value is an array");
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|l| l["receiptKey"] == data["key"]));
    let receipt_key = data["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

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
    assert_eq!(
        fetched["data"]["lines"]
            .as_array()
            .expect("the value is an array")
            .len(),
        2
    );
    let (status, listed) = send(
        router,
        "GET",
        &format!("/api/purchase-receipts?supplierKey={supplier_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        listed["data"]["items"]
            .as_array()
            .expect("the value is an array")
            .len(),
        1
    );
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
    let one_line = json!([{ "productKey": case_key, "quantity": 1, "lineTotalCents": 500, "sellingPriceCents": 900 }]);
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
                json!([{ "productKey": case_key, "quantity": 0, "lineTotalCents": 0, "sellingPriceCents": 2 }]),
                false,
            ),
        ),
        (
            "a product twice",
            receipt(
                None,
                json!([
                    { "productKey": case_key, "quantity": 1, "lineTotalCents": 1, "sellingPriceCents": 2 },
                    { "productKey": case_key, "quantity": 1, "lineTotalCents": 1, "sellingPriceCents": 2 },
                ]),
                false,
            ),
        ),
        (
            "serials on a plain product",
            receipt(
                None,
                json!([{ "productKey": case_key, "quantity": 1, "lineTotalCents": 1, "sellingPriceCents": 2, "serialNumbers": ["X"] }]),
                false,
            ),
        ),
        (
            "missing serials",
            receipt(
                None,
                json!([{ "productKey": phone_key, "quantity": 2, "lineTotalCents": 2, "sellingPriceCents": 2 }]),
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
    let (status, _) = send(router, "POST", "/api/purchase-receipts", Some(receipt(None, json!([{ "productKey": "prod_missing", "quantity": 1, "lineTotalCents": 1, "sellingPriceCents": 2 }]), false)), &admin).await;
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
        { "productKey": cable_key, "quantity": 5, "lineTotalCents": 500, "sellingPriceCents": 200 },
        { "productKey": "prod_missing", "quantity": 1, "lineTotalCents": 1, "sellingPriceCents": 2 },
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
