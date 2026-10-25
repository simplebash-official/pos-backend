// Lot prices and extra charges on supplier receipts: a line priced by its total (split into stock
// batches that add up exactly), shipping/duty/handling/insurance/discount charges spread over the
// lines into the stock cost, the ones the supplier bills changing what is owed and the ones paid
// to someone else (courier) paid at once. Runs unchanged on SQLite (the desktop) and on a
// throwaway Atlas database (the cloud).

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

struct Shop<'a> {
    router: &'a axum::Router,
    admin: String,
    supplier_key: String,
    terminal: String,
    session_key: String,
    courier_key: String,
    category_key: String,
    subcategory_key: String,
    tag: String,
}

impl Shop<'_> {
    async fn post(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        send(self.router, "POST", uri, Some(body), &self.admin).await
    }

    async fn get(&self, uri: &str) -> Value {
        let (status, body) = send(self.router, "GET", uri, None, &self.admin).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    /// A product; returns `(id, key)`.
    async fn product(&self, name: &str, serialized: bool) -> (String, String) {
        let (status, product) = self
            .post(
                "/api/inventory/products",
                json!({
                    "name": format!("{name} {}", self.tag),
                    "categoryKey": self.category_key,
                    "subcategoryKey": self.subcategory_key,
                    "minStockThreshold": 1,
                    "isSerialized": serialized,
                }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{product}");
        (
            product["data"]["id"].as_str().unwrap().to_string(),
            product["data"]["key"].as_str().unwrap().to_string(),
        )
    }

    async fn batches(&self, product_key: &str) -> Vec<Value> {
        let body = self
            .get(&format!("/api/inventory/products/{product_key}/batches"))
            .await;
        let mut items = body["data"]["items"].as_array().unwrap().clone();
        items.sort_by_key(|b| b["unitCostCents"].as_i64().unwrap());
        items
    }

    async fn expected_cash(&self) -> i64 {
        self.get(&format!("/api/sessions/{}", self.session_key)).await["data"]
            ["expectedCashCents"]
            .as_i64()
            .unwrap()
    }

    async fn receipts(&self) -> usize {
        self.get(&format!(
            "/api/purchase-receipts?supplierKey={}&limit=500",
            self.supplier_key
        ))
        .await["data"]["items"]
            .as_array()
            .unwrap()
            .len()
    }

    fn receipt(&self, lines: Value, charges: Value) -> Value {
        json!({
            "supplierKey": self.supplier_key,
            "date": chrono::Utc::now().to_rfc3339(),
            "lines": lines,
            "charges": charges,
        })
    }

    fn drawer_charges_payment(&self) -> Value {
        json!({ "source": "drawer", "terminalId": self.terminal, "sessionKey": self.session_key })
    }

    fn shipping_by_courier(&self, cents: i64) -> Value {
        json!({ "kind": "shipping", "amountCents": cents, "billedBy": "third_party", "payeeKey": self.courier_key })
    }
}

async fn scenario(app: &common::TestApp) {
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
        Some(json!({ "name": format!("Chg Cat {tag}"), "icon": "box", "color": "blue" })),
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
    let (_, supplier) = send(
        router,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": format!("Chg Supplier {tag}"), "contactPerson": "Nimal",
            "primaryPhone": "0771234567", "address": "Colombo", "suppliedCategories": [category_key_],
        })),
        &admin,
    )
    .await;
    let supplier_key = supplier["data"]["key"].as_str().unwrap().to_string();
    let (status, courier) = send(
        router,
        "POST",
        "/api/payees",
        Some(json!({ "payeeType": "vendor", "name": format!("Fast Courier {tag}") })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{courier}");
    let courier_key = courier["data"]["key"].as_str().unwrap().to_string();

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

    let shop = Shop {
        router,
        admin: admin.clone(),
        supplier_key,
        terminal,
        session_key,
        courier_key,
        category_key: category_key_,
        subcategory_key,
        tag,
    };
    let cod = category_key(
        CashMovementType::SupplierPayment,
        "Stock purchase (cash on delivery)",
    );

    // ---- A lot price that does not divide evenly: 30 pieces for Rs 50,000.00. ----
    let (_, lot_key) = shop.product("Lot item", false).await;
    let (status, lot) = shop
        .post(
            "/api/purchase-receipts",
            shop.receipt(
                json!([{ "productKey": lot_key, "quantity": 30, "lineTotalCents": 5_000_000, "sellingPriceCents": 200_000 }]),
                json!([]),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{lot}");
    let lot = &lot["data"];
    assert_eq!(
        (
            lot["goodsCents"].as_i64(),
            lot["extraChargesCents"].as_i64(),
            lot["totalCents"].as_i64(),
            lot["dueCents"].as_i64()
        ),
        (Some(5_000_000), Some(0), Some(5_000_000), Some(5_000_000)),
        "the bill is exactly the lot price"
    );
    let line = &lot["lines"][0];
    assert_eq!(line["lineTotalCents"], 5_000_000);
    assert_eq!(line["landedUnitCostCents"], 166_667);
    // Two batches a cent apart whose cost adds up to the lot price to the cent.
    let batches = shop.batches(&lot_key).await;
    let shape: Vec<(i64, i64)> = batches
        .iter()
        .map(|b| {
            (
                b["receivedQuantity"].as_i64().unwrap(),
                b["unitCostCents"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(shape, vec![(10, 166_666), (20, 166_667)]);
    assert_eq!(shape.iter().map(|(q, c)| q * c).sum::<i64>(), 5_000_000);

    // ---- Charges the supplier bills, spread over two lines; a discount takes money off. ----
    let (_, a_key) = shop.product("Charged A", false).await;
    let (_, b_key) = shop.product("Charged B", false).await;
    let two_lines = json!([
        { "productKey": a_key, "quantity": 10, "lineTotalCents": 60_000, "sellingPriceCents": 9_000 },
        { "productKey": b_key, "quantity": 10, "lineTotalCents": 40_000, "sellingPriceCents": 6_000 },
    ]);
    let (status, body) = shop
        .post(
            "/api/purchase-receipts",
            shop.receipt(
                two_lines.clone(),
                json!([{ "kind": "shipping", "amountCents": 2_000, "billedBy": "supplier" }]),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["code"], "ALLOCATION_REQUIRED",
        "with two products the spread must be chosen"
    );
    let (status, billed) = shop
        .post(
            "/api/purchase-receipts",
            shop.receipt(
                two_lines.clone(),
                json!([
                    { "kind": "shipping", "amountCents": 2_000, "billedBy": "supplier", "allocation": "value" },
                    { "kind": "discount", "amountCents": 500, "billedBy": "supplier", "allocation": "quantity" },
                ]),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{billed}");
    let billed = &billed["data"];
    assert_eq!(
        (
            billed["goodsCents"].as_i64(),
            billed["extraChargesCents"].as_i64(),
            billed["paidToOthersCents"].as_i64(),
            billed["totalCents"].as_i64(),
            billed["dueCents"].as_i64()
        ),
        (Some(100_000), Some(1_500), Some(0), Some(101_500), Some(101_500)),
        "what the supplier bills changes what is owed"
    );
    // Shipping 2,000 by value: 1,200 / 800. Discount 500 by quantity: 250 / 250. Shares: 950 / 550.
    let shares: Vec<i64> = billed["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["chargesShareCents"].as_i64().unwrap())
        .collect();
    assert_eq!(shares, vec![950, 550]);
    assert_eq!(billed["lines"][0]["landedTotalCents"], 60_950);
    assert_eq!(billed["charges"].as_array().unwrap().len(), 2);
    let a_cost: i64 = shop.batches(&a_key).await.iter().map(|b| b["receivedQuantity"].as_i64().unwrap() * b["unitCostCents"].as_i64().unwrap()).sum();
    assert_eq!(a_cost, 60_950, "the stock is costed with its share of the charges");

    // ---- A courier paid at once from the drawer: stock cost, not owed to the supplier. ----
    let (_, c_key) = shop.product("Courier item", false).await;
    let one_line = json!([{ "productKey": c_key, "quantity": 4, "lineTotalCents": 20_000, "sellingPriceCents": 8_000 }]);
    let cash_before = shop.expected_cash().await;
    let mut body = shop.receipt(one_line.clone(), json!([shop.shipping_by_courier(700)]));
    body["chargesPayment"] = shop.drawer_charges_payment();
    let (status, courier) = shop.post("/api/purchase-receipts", body).await;
    assert_eq!(status, StatusCode::CREATED, "{courier}");
    let courier = &courier["data"];
    assert_eq!(
        (
            courier["totalCents"].as_i64(),
            courier["dueCents"].as_i64(),
            courier["paidToOthersCents"].as_i64()
        ),
        (Some(20_000), Some(20_000), Some(700)),
        "a charge paid to someone else is not owed to the supplier"
    );
    assert_eq!(courier["settlements"], json!([]), "no supplier settlement");
    assert_eq!(courier["charges"][0]["paymentKind"], "cash_movement");
    assert_eq!(shop.expected_cash().await, cash_before - 700);
    assert_eq!(courier["lines"][0]["landedTotalCents"], 20_700);
    let batch_cost: i64 = shop.batches(&c_key).await.iter().map(|b| b["receivedQuantity"].as_i64().unwrap() * b["unitCostCents"].as_i64().unwrap()).sum();
    assert_eq!(batch_cost, 20_700);

    // ... and from the bank, through the expense ledger, with no register involved.
    let mut body = shop.receipt(one_line.clone(), json!([shop.shipping_by_courier(900)]));
    body["chargesPayment"] = json!({ "source": "bank", "paidOn": "2026-03-02" });
    let (status, bank) = shop.post("/api/purchase-receipts", body).await;
    assert_eq!(status, StatusCode::CREATED, "{bank}");
    assert_eq!(bank["data"]["charges"][0]["paymentKind"], "expense_payment");
    assert_eq!(shop.expected_cash().await, cash_before - 700, "the bank leaves the drawer alone");

    // ---- The goods payment and the courier together count against the drawer. ----
    // Each fits in the Rs 500 float on its own; together they do not.
    let drawer = shop.expected_cash().await;
    let each = drawer / 2 + 1_000;
    let receipts_before = shop.receipts().await;
    let mut body = shop.receipt(
        json!([{ "productKey": c_key, "quantity": 1, "lineTotalCents": each * 2, "sellingPriceCents": 9_000 }]),
        json!([shop.shipping_by_courier(each)]),
    );
    body["chargesPayment"] = shop.drawer_charges_payment();
    body["payment"] = json!({
        "source": "drawer", "amountCents": each, "categoryKey": cod,
        "terminalId": shop.terminal, "sessionKey": shop.session_key,
    });
    let stock_before = shop.batches(&c_key).await.len();
    let (status, refused) = shop.post("/api/purchase-receipts", body).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(refused["code"], "DRAWER_CASH_EXCEEDED");
    assert_eq!(shop.receipts().await, receipts_before, "a refused save writes no receipt");
    assert_eq!(shop.batches(&c_key).await.len(), stock_before, "and no stock");
    assert_eq!(shop.expected_cash().await, drawer, "and moves no cash");

    // ---- Refused requests. ----
    let courier = shop.shipping_by_courier(500);
    let pay = shop.drawer_charges_payment();
    let cases: Vec<(&str, Value)> = vec![
        ("zero amount", {
            let mut b = shop.receipt(one_line.clone(), json!([{ "kind": "other", "amountCents": 0, "billedBy": "supplier" }]));
            b["charges"][0]["amountCents"] = json!(0);
            b
        }),
        ("a discount cannot be paid to someone else", shop.receipt(one_line.clone(), json!([{ "kind": "discount", "amountCents": 100, "billedBy": "third_party", "payeeKey": shop.courier_key }]))),
        ("a supplier's own charge has no payee", shop.receipt(one_line.clone(), json!([{ "kind": "shipping", "amountCents": 100, "billedBy": "supplier", "payeeKey": shop.courier_key }]))),
        ("a third-party charge needs a payee", shop.receipt(one_line.clone(), json!([{ "kind": "shipping", "amountCents": 100, "billedBy": "third_party" }]))),
        ("one product has nothing to spread over", shop.receipt(one_line.clone(), json!([{ "kind": "shipping", "amountCents": 100, "billedBy": "supplier", "allocation": "value" }]))),
        ("a third-party charge needs a way to pay", shop.receipt(one_line.clone(), json!([courier.clone()]))),
        ("a payment with nothing to pay", {
            let mut b = shop.receipt(one_line.clone(), json!([{ "kind": "shipping", "amountCents": 100, "billedBy": "supplier" }]));
            b["chargesPayment"] = pay.clone();
            b
        }),
        ("a discount bigger than the goods", shop.receipt(one_line.clone(), json!([{ "kind": "discount", "amountCents": 20_000, "billedBy": "supplier" }]))),
        ("a supplier cannot be the payee", {
            let mut b = shop.receipt(one_line.clone(), json!([{ "kind": "shipping", "amountCents": 100, "billedBy": "third_party", "payeeKey": shop.supplier_key }]));
            b["chargesPayment"] = pay.clone();
            b
        }),
        ("a PIN with no cash", {
            let mut b = shop.receipt(one_line.clone(), json!([]));
            b["approval"] = json!({ "managerPin": "1234" });
            b
        }),
        ("lot total of zero", shop.receipt(json!([{ "productKey": c_key, "quantity": 2, "lineTotalCents": 0, "sellingPriceCents": 100 }]), json!([]))),
    ];
    for (what, body) in cases {
        let (status, response) = shop.post("/api/purchase-receipts", body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: {response}");
    }
    assert_eq!(shop.expected_cash().await, drawer, "refusals move no cash");

    // ---- A serialized product keeps every serial across a split lot. ----
    let (_, phone_key) = shop.product("Phone", true).await;
    let serials = ["IMEI-1", "IMEI-2", "IMEI-3"].map(|s| format!("{s}-{}", shop.tag));
    let (status, phones) = shop
        .post(
            "/api/purchase-receipts",
            shop.receipt(
                json!([{ "productKey": phone_key, "quantity": 3, "lineTotalCents": 100_000, "sellingPriceCents": 50_000, "serialNumbers": serials }]),
                json!([]),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{phones}");
    let phone_batches = shop.batches(&phone_key).await;
    let shape: Vec<(i64, i64)> = phone_batches
        .iter()
        .map(|b| (b["receivedQuantity"].as_i64().unwrap(), b["unitCostCents"].as_i64().unwrap()))
        .collect();
    assert_eq!(shape, vec![(2, 33_333), (1, 33_334)]);
    let listed = shop
        .get(&format!("/api/inventory/products/{phone_key}/serials"))
        .await;
    assert_eq!(listed["data"]["items"].as_array().unwrap().len(), 3);

    // ---- The supplier balance owes the goods and the charges the supplier billed, not the courier. ----
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let (status, set) = send(
        router,
        "PUT",
        &format!("/api/balances/supplier/{}/opening", shop.supplier_key),
        Some(json!({ "openingBalanceCents": 0, "asOfDate": today })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{set}");
    let balance = |body: &Value| {
        body["data"]["suppliers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["supplierKey"] == json!(shop.supplier_key))
            .map(|row| row["balanceCents"].as_i64().unwrap())
            .unwrap()
    };
    let before = balance(&shop.get("/api/balances/suppliers").await);
    let mut body = shop.receipt(
        two_lines,
        json!([
            { "kind": "shipping", "amountCents": 3_000, "billedBy": "supplier", "allocation": "value" },
            { "kind": "handling", "amountCents": 800, "billedBy": "third_party", "payeeKey": shop.courier_key, "allocation": "quantity" },
        ]),
    );
    body["chargesPayment"] = json!({ "source": "bank", "paidOn": "2026-03-02" });
    let (status, owed) = shop.post("/api/purchase-receipts", body).await;
    assert_eq!(status, StatusCode::CREATED, "{owed}");
    assert_eq!(owed["data"]["totalCents"], 103_000);
    let after = balance(&shop.get("/api/balances/suppliers").await);
    assert_eq!(
        after - before,
        103_000,
        "the balance and the receipt agree on what is owed"
    );
}

#[tokio::test]
async fn charges_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn charges_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}
