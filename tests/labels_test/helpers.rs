//! Shared setup for the `labels_test` tests.

use crate::common;
use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::domain::users::Role;
use tower::ServiceExt;

// End-to-end coverage of label printing through the real router: the starter
// and shop-built designs, printing products and serial numbers into stickers,
// creating a barcode for a product that has none, scanning a serial number, the
// rules that keep an unreadable or oversized print from ever being sent, and the
// print history (`/labels/jobs`) with each product's label state
// (`/labels/coverage`). Printing is for Admin and Manager only.
// The main scenario runs unchanged on both engines: SQLite (the desktop) and a
// throwaway Atlas database (the cloud), dropped at the end. The mock
// document-server echoes the JSON it was asked to render, which is how these
// tests see exactly what the backend sent.

pub(crate) async fn send_raw(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Vec<u8>) {
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
    (status, bytes.to_vec())
}

pub(crate) async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let (status, bytes) = send_raw(router, method, uri, body, token).await;
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json)
}

/// The JSON the backend handed to document-server, read back from the mock's
/// echoed PDF text.
pub(crate) fn rendered_payload(pdf: &[u8]) -> Value {
    let text = String::from_utf8_lossy(pdf);
    assert!(text.starts_with("%PDF-"), "not a PDF: {text}");
    assert!(
        text.contains("template tpl_product_label "),
        "rendered with the wrong template: {text}"
    );
    let (_, json) = text.split_once(" body=").expect("mock echoes its body");
    serde_json::from_str(json).expect("the sent payload is JSON")
}

pub(crate) fn token(app: &common::TestApp, role: Role) -> String {
    common::mint_token(&app.config, Some(role), roles::default_permissions(role))
}

pub(crate) struct Shop {
    pub(crate) router: axum::Router,
    pub(crate) admin: String,
    pub(crate) manager: String,
    pub(crate) staff: String,
    pub(crate) category_key: String,
    pub(crate) subcategory_key: String,
    pub(crate) supplier_key: String,
    pub(crate) tag: String,
}

impl Shop {
    pub(crate) async fn new(app: &common::TestApp) -> Shop {
        let router = app.router.clone();
        let admin = token(app, Role::Admin);
        let tag = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();

        let (status, category) = send(
            &router,
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": format!("Label Cat {tag}"), "icon": "box", "color": "blue" })),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{category}");
        let category_key = category["data"]["key"]
            .as_str()
            .expect("the value is a string")
            .to_string();
        let (status, sub) = send(
            &router,
            "POST",
            &format!("/api/inventory/categories/{category_key}/subcategories"),
            Some(json!({ "name": "Items" })),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{sub}");
        let subcategory_key = sub["data"]["subcategories"][0]["key"]
            .as_str()
            .expect("the value is a string")
            .to_string();
        let (status, supplier) = send(
            &router,
            "POST",
            "/api/suppliers",
            Some(json!({
                "name": format!("Label Supplier {tag}"),
                "contactPerson": "Nimal",
                "primaryPhone": "0771234567",
                "address": "Colombo",
                "suppliedCategories": [category_key],
            })),
            &admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{supplier}");

        Shop {
            router,
            manager: token(app, Role::Manager),
            staff: token(app, Role::Staff),
            admin,
            category_key,
            subcategory_key,
            supplier_key: supplier["data"]["key"]
                .as_str()
                .expect("the value is a string")
                .to_string(),
            tag,
        }
    }

    pub(crate) async fn product(&self, name: &str, extra: Value) -> Value {
        let mut body = json!({
            "name": format!("{name} {}", self.tag),
            "categoryKey": self.category_key,
            "subcategoryKey": self.subcategory_key,
            "minStockThreshold": 1,
        });
        for (key, value) in extra.as_object().expect("the value is an object") {
            body[key] = value.clone();
        }
        let (status, product) = send(
            &self.router,
            "POST",
            "/api/inventory/products",
            Some(body),
            &self.admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{product}");
        product["data"].clone()
    }

    pub(crate) async fn purchase(
        &self,
        product_key: &str,
        quantity: i64,
        price: i64,
        serials: Option<Vec<String>>,
    ) {
        let mut line = json!({
            "productKey": product_key,
            "quantity": quantity,
            "lineTotalCents": quantity * (price / 2),
            "sellingPriceCents": price,
        });
        if let Some(serials) = serials {
            line["serialNumbers"] = json!(serials);
        }
        let body = json!({
            "supplierKey": self.supplier_key,
            "date": chrono::Utc::now().to_rfc3339(),
            "lines": [line],
        });
        let (status, response) = send(
            &self.router,
            "POST",
            "/api/purchase-receipts",
            Some(body),
            &self.admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{response}");
    }

    pub(crate) async fn render(&self, template_key: &str, items: Value) -> (StatusCode, Vec<u8>) {
        send_raw(
            &self.router,
            "POST",
            "/api/labels/render",
            Some(json!({ "templateKey": template_key, "items": items })),
            &self.manager,
        )
        .await
    }

    /// Records a print job; returns the status, the job key header and the body.
    pub(crate) async fn print_job(&self, body: Value) -> (StatusCode, Option<String>, Vec<u8>) {
        let request = Request::builder()
            .method("POST")
            .uri("/api/labels/jobs")
            .header(AUTHORIZATION, format!("Bearer {}", self.manager))
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("the request is well formed");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("the awaited step succeeds");
        let status = response.status();
        let key = response
            .headers()
            .get("x-label-job-key")
            .map(|v| v.to_str().expect("the header is text").to_string());
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the awaited step succeeds");
        (status, key, bytes.to_vec())
    }

    pub(crate) async fn get(&self, uri: &str) -> Value {
        let (status, body) = send(&self.router, "GET", uri, None, &self.manager).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        body["data"].clone()
    }

    pub(crate) async fn render_json(
        &self,
        template_key: &str,
        items: Value,
    ) -> (StatusCode, Value) {
        let (status, bytes) = self.render(template_key, items).await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

pub(crate) const PRICE_TAG: &str = "starter_price_tag_50x30";

pub(crate) fn design(name: &str, elements: Value) -> Value {
    json!({ "name": name, "widthMm": 50, "heightMm": 30, "elements": elements })
}

pub(crate) fn text(id: &str, source: &str, custom: Value) -> Value {
    json!({
        "id": id, "type": "text", "source": source, "customText": custom,
        "xMm": 2, "yMm": 2, "widthMm": 40, "heightMm": 6,
        "fontSizePt": 8, "bold": false, "align": "left"
    })
}

pub(crate) fn barcode(width: f64) -> Value {
    json!({
        "id": "bc", "type": "barcode", "xMm": 1, "yMm": 10,
        "widthMm": width, "heightMm": 15, "showValue": true, "fontSizePt": 6
    })
}

pub(crate) async fn scenario(app: &common::TestApp) {
    let shop = Shop::new(app).await;

    // ---- designs: starters first, then CRUD with unique names ----------------
    let (status, list) = send(
        &shop.router,
        "GET",
        "/api/labels/templates",
        None,
        &shop.staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let templates = list["data"]["templates"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(templates.len(), 3);
    assert!(templates.iter().all(|t| t["builtIn"] == true));
    assert!(templates[0].get("createdAt").is_none());

    let (status, created) = send(
        &shop.router,
        "POST",
        "/api/labels/templates",
        Some(design("Barcode only", json!([barcode(46.0)]))),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let barcode_only = created["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert!(barcode_only.starts_with("ltpl_"));
    assert_eq!(created["data"]["builtIn"], false);

    for name in ["barcode only", "  Price tag 50 x 30 mm "] {
        let (status, body) = send(
            &shop.router,
            "POST",
            "/api/labels/templates",
            Some(design(name, json!([barcode(46.0)]))),
            &shop.manager,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{name}: {body}");
        assert_eq!(body["code"], "LABEL_TEMPLATE_NAME_TAKEN");
    }
    for bad in [
        design("Empty", json!([])),
        design("Too wide", json!([barcode(60.0)])),
        design("No text", json!([text("t", "customText", Value::Null)])),
    ] {
        let (status, body) = send(
            &shop.router,
            "POST",
            "/api/labels/templates",
            Some(bad),
            &shop.manager,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }

    let (status, updated) = send(
        &shop.router,
        "PUT",
        &format!("/api/labels/templates/{barcode_only}"),
        Some(design("Barcode only v2", json!([barcode(46.0)]))),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["data"]["name"], "Barcode only v2");
    // Renaming a design to its own current name is not a clash.
    let (status, _) = send(
        &shop.router,
        "PUT",
        &format!("/api/labels/templates/{barcode_only}"),
        Some(design("Barcode only v2", json!([barcode(46.0)]))),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    for (method, uri) in [("PUT", PRICE_TAG), ("DELETE", PRICE_TAG)] {
        let (status, body) = send(
            &shop.router,
            method,
            &format!("/api/labels/templates/{uri}"),
            (method == "PUT").then(|| design("Edited", json!([barcode(46.0)]))),
            &shop.manager,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["code"], "LABEL_TEMPLATE_BUILT_IN");
    }
    let (status, body) = send(
        &shop.router,
        "PUT",
        "/api/labels/templates/ltpl_missing",
        Some(design("Ghost", json!([barcode(46.0)]))),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "LABEL_TEMPLATE_NOT_FOUND");

    // ---- products: a barcode, two prices, no barcode, serialized -------------
    let with_barcode = shop
        .product("Cable", json!({ "autoGenerateBarcode": true }))
        .await;
    let cable_key = with_barcode["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let cable_code = with_barcode["barcode"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    shop.purchase(&cable_key, 5, 150_000, None).await;
    shop.purchase(&cable_key, 5, 175_000, None).await;

    let no_barcode = shop.product("Charger", json!({})).await;
    let charger_key = no_barcode["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let charger_id = no_barcode["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    shop.purchase(&charger_key, 3, 90_000, None).await;

    let serialized = shop.product("Phone", json!({ "isSerialized": true })).await;
    let phone_key = serialized["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    // IMEI-shaped (15 digits), unique per run: serial numbers are global.
    let imei = uuid::Uuid::new_v4().as_u128() % 100_000_000;
    let serials: Vec<String> = (1..=2).map(|i| format!("356938{imei:08}{i}")).collect();
    shop.purchase(&phone_key, 2, 80_000, Some(serials.clone()))
        .await;

    // ---- printing a product: everything resolved server-side -----------------
    let (status, pdf) = shop
        .render(
            PRICE_TAG,
            json!([{ "kind": "product", "productKey": cable_key, "copies": 3, "priceCents": 175_000 }]),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pdf));
    let payload = rendered_payload(&pdf);
    assert_eq!(payload["widthMm"], 50.0);
    assert_eq!(payload["heightMm"], 30.0);
    assert_eq!(
        payload["elements"]
            .as_array()
            .expect("the value is an array")
            .len(),
        3
    );
    assert_eq!(
        payload["labels"]
            .as_array()
            .expect("the value is an array")
            .len(),
        1
    );
    let sticker = &payload["labels"][0];
    assert_eq!(sticker["code"], cable_code.as_str());
    assert_eq!(sticker["copies"], 3);
    assert_eq!(sticker["priceCents"], 175_000);
    assert_eq!(sticker["name"], format!("Cable {}", shop.tag).as_str());
    assert_eq!(
        sticker["category"],
        format!("Label Cat {}", shop.tag).as_str()
    );
    assert!(
        sticker["sku"]
            .as_str()
            .expect("the value is a string")
            .contains('-')
    );
    assert!(payload["shopName"].is_string());

    // A saved design prints the same way; one without a price needs none.
    let (status, pdf) = shop
        .render(
            &barcode_only,
            json!([{ "kind": "product", "productKey": cable_key, "copies": 1, "priceCents": null }]),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pdf));
    assert!(rendered_payload(&pdf)["labels"][0]["priceCents"].is_null());

    // ---- refusals --------------------------------------------------------------
    let item = |key: &str, copies: i64, price: Value| json!({ "kind": "product", "productKey": key, "copies": copies, "priceCents": price });
    let (status, body) = shop
        .render_json(PRICE_TAG, json!([item(&cable_key, 1, Value::Null)]))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "LABEL_PRICE_REQUIRED");
    let (status, body) = shop
        .render_json(PRICE_TAG, json!([item(&cable_key, 1, json!(1))]))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "PRICE_TIER_UNAVAILABLE");
    let (status, body) = shop
        .render_json(PRICE_TAG, json!([item("prod_missing", 1, Value::Null)]))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = shop
        .render_json("ltpl_missing", json!([item(&cable_key, 1, json!(150_000))]))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "LABEL_TEMPLATE_NOT_FOUND");
    for (items, why) in [
        (json!([]), "nothing chosen"),
        (json!([item(&cable_key, 0, json!(150_000))]), "zero copies"),
        (
            json!([item(&cable_key, 501, json!(150_000))]),
            "too many copies",
        ),
        (
            json!([
                item(&cable_key, 300, json!(150_000)),
                item(&cable_key, 300, json!(175_000))
            ]),
            "over the job limit",
        ),
    ] {
        let (status, body) = shop.render_json(PRICE_TAG, items).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}: {body}");
    }
    let many: Vec<Value> = (0..201)
        .map(|_| item(&cable_key, 1, json!(150_000)))
        .collect();
    let (status, body) = shop.render_json(PRICE_TAG, Value::Array(many)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // Exactly at the limits is fine.
    let (status, _) = shop
        .render(PRICE_TAG, json!([item(&cable_key, 500, json!(150_000))]))
        .await;
    assert_eq!(status, StatusCode::OK);

    // A barcode too narrow to scan is refused rather than printed.
    let (status, narrow) = send(
        &shop.router,
        "POST",
        "/api/labels/templates",
        Some(design("Narrow", json!([barcode(25.0)]))),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{narrow}");
    let (status, body) = shop
        .render_json(
            narrow["data"]["key"]
                .as_str()
                .expect("the value is a string"),
            json!([item(&cable_key, 1, Value::Null)]),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "LABEL_BARCODE_TOO_NARROW");

    // ---- a product with no barcode: refused, then given one -------------------
    let (status, body) = shop
        .render_json(PRICE_TAG, json!([item(&charger_key, 1, json!(90_000))]))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "LABEL_PRODUCT_WITHOUT_BARCODE");

    let (_, before) = send(
        &shop.router,
        "GET",
        &format!("/api/inventory/products/{charger_id}"),
        None,
        &shop.admin,
    )
    .await;
    let version_before = before["data"]["version"]
        .as_i64()
        .expect("the value is an integer");
    let (status, generated) = send(
        &shop.router,
        "POST",
        &format!("/api/inventory/products/{charger_id}/barcode"),
        None,
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{generated}");
    let new_code = generated["data"]["barcode"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert_eq!(new_code.len(), 13);
    assert!(new_code.starts_with("20") && new_code.chars().all(|c| c.is_ascii_digit()));
    assert_eq!(generated["data"]["barcodeSource"], "generated");
    // The write bumps the version (so it syncs) and a Manager never sees cost.
    assert_eq!(generated["data"]["version"], version_before + 1);
    assert!(
        generated["data"]["priceTiers"][0]
            .get("minUnitCostCents")
            .is_none()
    );

    let (status, body) = send(
        &shop.router,
        "POST",
        &format!("/api/inventory/products/{charger_id}/barcode"),
        None,
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "PRODUCT_HAS_BARCODE");
    let (status, body) = send(
        &shop.router,
        "POST",
        "/api/inventory/products/64b0000000000000000000ff/barcode",
        None,
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    let (status, pdf) = shop
        .render(PRICE_TAG, json!([item(&charger_key, 2, json!(90_000))]))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pdf));
    assert_eq!(
        rendered_payload(&pdf)["labels"][0]["code"],
        new_code.as_str()
    );

    // ---- serial labels: the unit's own number at its own batch's price --------
    let serial = &serials[0];
    let (status, pdf) = shop
        .render(
            PRICE_TAG,
            json!([{ "kind": "serial", "serialNumber": serial, "copies": 2 }]),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pdf));
    let sticker = &rendered_payload(&pdf)["labels"][0];
    assert_eq!(sticker["code"], serial.as_str());
    assert_eq!(sticker["priceCents"], 80_000);
    assert_eq!(sticker["copies"], 2);
    assert_eq!(sticker["name"], format!("Phone {}", shop.tag).as_str());

    let (status, lookup) = send(
        &shop.router,
        "GET",
        &format!("/api/inventory/serials/by-number/{serial}"),
        None,
        &shop.staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{lookup}");
    assert_eq!(lookup["data"]["serial"]["serialNumber"], serial.as_str());
    assert_eq!(lookup["data"]["product"]["key"], phone_key.as_str());
    assert_eq!(lookup["data"]["sellingPriceCents"], 80_000);
    assert!(
        lookup["data"]["product"]["priceTiers"][0]
            .get("minUnitCostCents")
            .is_none()
    );
    let (status, body) = send(
        &shop.router,
        "GET",
        "/api/inventory/serials/by-number/NOPE-123",
        None,
        &shop.staff,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "SERIAL_NOT_FOUND");

    // Once sold, the unit is neither scannable nor printable.
    let (status, sale) = send(
        &shop.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier" },
            "items": [{
                "productKey": phone_key, "quantity": 1, "discountCents": 0,
                "sourceType": "retail", "serialNumbers": [serial],
            }],
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 80_000 },
            "shopProfileSnapshot": { "tradingName": "Label Shop" },
        })),
        &shop.admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sale}");
    let (status, body) = send(
        &shop.router,
        "GET",
        &format!("/api/inventory/serials/by-number/{serial}"),
        None,
        &shop.staff,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "SERIAL_ALREADY_SOLD");
    let (status, body) = shop
        .render_json(
            PRICE_TAG,
            json!([{ "kind": "serial", "serialNumber": serial, "copies": 1 }]),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // ---- preview of an unsaved design -------------------------------------------
    let sample = json!({
        "name": "Sample item", "sku": "SKU-0001", "category": "Samples",
        "priceCents": 125_000, "code": "2000000000015"
    });
    let (status, pdf) = send_raw(
        &shop.router,
        "POST",
        "/api/labels/preview",
        Some(json!({ "template": design("Unsaved", json!([barcode(46.0)])), "sample": sample })),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pdf));
    assert_eq!(rendered_payload(&pdf)["labels"][0]["name"], "Sample item");
    let (status, body) = send(
        &shop.router,
        "POST",
        "/api/labels/preview",
        Some(json!({ "template": design("Unsaved", json!([])), "sample": sample })),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = send(
        &shop.router,
        "POST",
        "/api/labels/preview",
        Some(json!({ "template": design("Unsaved", json!([barcode(25.0)])), "sample": sample })),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // ---- delete ---------------------------------------------------------------------
    let uri = format!("/api/labels/templates/{barcode_only}");
    let (status, _) = send(&shop.router, "DELETE", &uri, None, &shop.manager).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = send(&shop.router, "DELETE", &uri, None, &shop.manager).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (_, list) = send(
        &shop.router,
        "GET",
        "/api/labels/templates",
        None,
        &shop.staff,
    )
    .await;
    let names: Vec<&str> = list["data"]["templates"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|t| t["name"].as_str().expect("the value is a string"))
        .collect();
    assert!(!names.contains(&"Barcode only v2"));
    assert!(names.contains(&"Narrow"));
}
