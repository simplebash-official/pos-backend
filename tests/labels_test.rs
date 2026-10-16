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

async fn send_raw(
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
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

async fn send(
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
fn rendered_payload(pdf: &[u8]) -> Value {
    let text = String::from_utf8_lossy(pdf);
    assert!(text.starts_with("%PDF-"), "not a PDF: {text}");
    assert!(
        text.contains("template tpl_product_label "),
        "rendered with the wrong template: {text}"
    );
    let (_, json) = text.split_once(" body=").expect("mock echoes its body");
    serde_json::from_str(json).expect("the sent payload is JSON")
}

fn token(app: &common::TestApp, role: Role) -> String {
    common::mint_token(&app.config, Some(role), roles::default_permissions(role))
}

struct Shop {
    router: axum::Router,
    admin: String,
    manager: String,
    staff: String,
    category_key: String,
    subcategory_key: String,
    supplier_key: String,
    tag: String,
}

impl Shop {
    async fn new(app: &common::TestApp) -> Shop {
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
        let category_key = category["data"]["key"].as_str().unwrap().to_string();
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
            .unwrap()
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
            supplier_key: supplier["data"]["key"].as_str().unwrap().to_string(),
            tag,
        }
    }

    async fn product(&self, name: &str, extra: Value) -> Value {
        let mut body = json!({
            "name": format!("{name} {}", self.tag),
            "categoryKey": self.category_key,
            "subcategoryKey": self.subcategory_key,
            "minStockThreshold": 1,
        });
        for (key, value) in extra.as_object().unwrap() {
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

    async fn purchase(
        &self,
        product_key: &str,
        quantity: i64,
        price: i64,
        serials: Option<Vec<String>>,
    ) {
        let mut body = json!({
            "supplierKey": self.supplier_key,
            "productKey": product_key,
            "quantity": quantity,
            "unitCostCents": price / 2,
            "sellingPriceCents": price,
            "date": chrono::Utc::now().to_rfc3339(),
        });
        if let Some(serials) = serials {
            body["serialNumbers"] = json!(serials);
        }
        let (status, response) = send(
            &self.router,
            "POST",
            "/api/purchases",
            Some(body),
            &self.admin,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{response}");
    }

    async fn render(&self, template_key: &str, items: Value) -> (StatusCode, Vec<u8>) {
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
    async fn print_job(&self, body: Value) -> (StatusCode, Option<String>, Vec<u8>) {
        let request = Request::builder()
            .method("POST")
            .uri("/api/labels/jobs")
            .header(AUTHORIZATION, format!("Bearer {}", self.manager))
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let key = response
            .headers()
            .get("x-label-job-key")
            .map(|v| v.to_str().unwrap().to_string());
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, key, bytes.to_vec())
    }

    async fn get(&self, uri: &str) -> Value {
        let (status, body) = send(&self.router, "GET", uri, None, &self.manager).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        body["data"].clone()
    }

    async fn render_json(&self, template_key: &str, items: Value) -> (StatusCode, Value) {
        let (status, bytes) = self.render(template_key, items).await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

const PRICE_TAG: &str = "starter_price_tag_50x30";

fn design(name: &str, elements: Value) -> Value {
    json!({ "name": name, "widthMm": 50, "heightMm": 30, "elements": elements })
}

fn text(id: &str, source: &str, custom: Value) -> Value {
    json!({
        "id": id, "type": "text", "source": source, "customText": custom,
        "xMm": 2, "yMm": 2, "widthMm": 40, "heightMm": 6,
        "fontSizePt": 8, "bold": false, "align": "left"
    })
}

fn barcode(width: f64) -> Value {
    json!({
        "id": "bc", "type": "barcode", "xMm": 1, "yMm": 10,
        "widthMm": width, "heightMm": 15, "showValue": true, "fontSizePt": 6
    })
}

async fn scenario(app: &common::TestApp) {
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
    let templates = list["data"]["templates"].as_array().unwrap();
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
    let barcode_only = created["data"]["key"].as_str().unwrap().to_string();
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
    let cable_key = with_barcode["key"].as_str().unwrap().to_string();
    let cable_code = with_barcode["barcode"].as_str().unwrap().to_string();
    shop.purchase(&cable_key, 5, 150_000, None).await;
    shop.purchase(&cable_key, 5, 175_000, None).await;

    let no_barcode = shop.product("Charger", json!({})).await;
    let charger_key = no_barcode["key"].as_str().unwrap().to_string();
    let charger_id = no_barcode["id"].as_str().unwrap().to_string();
    shop.purchase(&charger_key, 3, 90_000, None).await;

    let serialized = shop.product("Phone", json!({ "isSerialized": true })).await;
    let phone_key = serialized["key"].as_str().unwrap().to_string();
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
    assert_eq!(payload["elements"].as_array().unwrap().len(), 3);
    assert_eq!(payload["labels"].as_array().unwrap().len(), 1);
    let sticker = &payload["labels"][0];
    assert_eq!(sticker["code"], cable_code.as_str());
    assert_eq!(sticker["copies"], 3);
    assert_eq!(sticker["priceCents"], 175_000);
    assert_eq!(sticker["name"], format!("Cable {}", shop.tag).as_str());
    assert_eq!(
        sticker["category"],
        format!("Label Cat {}", shop.tag).as_str()
    );
    assert!(sticker["sku"].as_str().unwrap().contains('-'));
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
            narrow["data"]["key"].as_str().unwrap(),
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
    let version_before = before["data"]["version"].as_i64().unwrap();
    let (status, generated) = send(
        &shop.router,
        "POST",
        &format!("/api/inventory/products/{charger_id}/barcode"),
        None,
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{generated}");
    let new_code = generated["data"]["barcode"].as_str().unwrap().to_string();
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
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&"Barcode only v2"));
    assert!(names.contains(&"Narrow"));
}

#[tokio::test]
async fn labels_scenario_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn labels_scenario_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}

/// Every role that can see the catalog may read the designs; making or changing
/// designs, previewing, printing, the print history and creating a barcode need
/// inventory write (Admin and Manager).
#[tokio::test]
async fn labels_follow_the_inventory_permissions() {
    let app = common::spawn_app_sqlite().await;
    let shop = Shop::new(&app).await;
    let (status, list) = send(
        &shop.router,
        "GET",
        "/api/labels/templates",
        None,
        &shop.staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list}");

    let valid = design("Staff attempt", json!([barcode(46.0)]));
    let sample = json!({
        "name": "n", "sku": "s", "category": "c", "priceCents": 100, "code": "2000000000015"
    });
    for (method, uri, body) in [
        ("POST", "/api/labels/templates", Some(valid.clone())),
        ("PUT", "/api/labels/templates/ltpl_x", Some(valid.clone())),
        ("DELETE", "/api/labels/templates/ltpl_x", None),
        (
            "POST",
            "/api/labels/preview",
            Some(json!({ "template": valid, "sample": sample })),
        ),
        (
            "POST",
            "/api/inventory/products/64b0000000000000000000ff/barcode",
            None,
        ),
        (
            "POST",
            "/api/labels/render",
            Some(json!({ "templateKey": PRICE_TAG, "items": [] })),
        ),
        (
            "POST",
            "/api/labels/jobs",
            Some(
                json!({ "templateKey": PRICE_TAG, "items": [], "action": "download",
                         "method": null, "printerName": null, "dpi": null }),
            ),
        ),
        ("GET", "/api/labels/jobs", None),
        ("GET", "/api/labels/jobs/lpj_x", None),
        (
            "PATCH",
            "/api/labels/jobs/lpj_x",
            Some(json!({ "outcome": "sent", "error": null })),
        ),
        ("GET", "/api/labels/coverage", None),
    ] {
        let (status, body) = send(&shop.router, method, uri, body, &shop.staff).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {body}");
    }
}

#[tokio::test]
async fn each_shop_has_its_own_label_designs() {
    let app = common::spawn_app_multi_tenant().await;
    let admin_of = |tenant: &str| {
        common::mint_token_for_tenant(
            &app.config,
            Some(Role::Admin),
            roles::default_permissions(Role::Admin),
            Some(tenant),
        )
    };
    let (shop_a, shop_b) = (admin_of("tnt_label_a"), admin_of("tnt_label_b"));

    let (status, body) = send(
        &app.router,
        "POST",
        "/api/labels/templates",
        Some(design("Only for A", json!([barcode(46.0)]))),
        &shop_a,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let key = body["data"]["key"].as_str().unwrap().to_string();

    let names = |list: &Value| -> Vec<String> {
        list["data"]["templates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    };
    let (_, a) = send(&app.router, "GET", "/api/labels/templates", None, &shop_a).await;
    let (_, b) = send(&app.router, "GET", "/api/labels/templates", None, &shop_b).await;
    assert!(names(&a).contains(&"Only for A".to_string()));
    assert!(
        !names(&b).contains(&"Only for A".to_string()),
        "shop B must not see shop A's design"
    );

    // Shop B can neither change nor delete it, and may reuse the name.
    let uri = format!("/api/labels/templates/{key}");
    let (status, _) = send(&app.router, "DELETE", &uri, None, &shop_b).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        &app.router,
        "POST",
        "/api/labels/templates",
        Some(design("Only for A", json!([barcode(46.0)]))),
        &shop_b,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    app.db.drop().await.ok();
}

/// Prints, downloads and failed prints are recorded with what, how and who; the
/// coverage view tells never labelled, labels needed for new stock, and labelled.
async fn jobs_scenario(app: &common::TestApp) {
    let shop = Shop::new(app).await;
    let cable = shop
        .product("Cable", json!({ "autoGenerateBarcode": true }))
        .await;
    let cable_key = cable["key"].as_str().unwrap().to_string();
    shop.purchase(&cable_key, 5, 150_000, None).await;
    let charger = shop
        .product("Charger", json!({ "autoGenerateBarcode": true }))
        .await;
    let charger_key = charger["key"].as_str().unwrap().to_string();
    shop.purchase(&charger_key, 2, 90_000, None).await;
    let empty = shop
        .product("Empty shelf", json!({ "autoGenerateBarcode": true }))
        .await;
    let phone = shop.product("Phone", json!({ "isSerialized": true })).await;
    let phone_key = phone["key"].as_str().unwrap().to_string();
    let imei = uuid::Uuid::new_v4().as_u128() % 100_000_000;
    let serials: Vec<String> = (1..=2).map(|i| format!("356938{imei:08}{i}")).collect();
    shop.purchase(&phone_key, 2, 80_000, Some(serials.clone()))
        .await;

    let search = format!("/api/labels/coverage?search={}", shop.tag);
    let row_of = |coverage: &Value, key: &str| -> Value {
        coverage["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["product"]["key"] == key)
            .cloned()
            .unwrap_or_else(|| panic!("{key} missing from {coverage}"))
    };

    // ---- before any print: in-stock items were never labelled ------------------
    let coverage = shop.get(&search).await;
    assert_eq!(coverage["counts"]["never"], 3, "{coverage}");
    assert_eq!(coverage["counts"]["needed"], 0);
    assert_eq!(coverage["counts"]["labelled"], 0);
    assert_eq!(row_of(&coverage, &cable_key)["status"], "never");
    assert!(row_of(&coverage, &cable_key)["lastLabelledAt"].is_null());
    // Nothing in stock and never labelled: nothing to label, listed last.
    let empty_row = row_of(&coverage, empty["key"].as_str().unwrap());
    assert!(empty_row["status"].is_null());
    assert_eq!(
        coverage["items"].as_array().unwrap().last().unwrap()["product"]["key"],
        empty["key"]
    );

    // ---- searching narrows the list, the same on SQLite and MongoDB ------------
    let charger_search = format!("/api/labels/coverage?search=Charger%20{}", shop.tag);
    let found = shop.get(&charger_search).await;
    assert_eq!(found["pagination"]["total"], 1, "{found}");
    assert_eq!(found["items"][0]["product"]["key"], charger_key.as_str());
    let (status, products) = send(
        &shop.router,
        "GET",
        &format!("/api/inventory/products?search=charger%20{}", shop.tag),
        None,
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{products}");
    assert_eq!(products["data"]["pagination"]["total"], 1, "{products}");
    // A wildcard typed by the person is just a character.
    let none = shop
        .get(&format!("/api/labels/coverage?search=Cab%25{}", shop.tag))
        .await;
    assert_eq!(none["pagination"]["total"], 0, "{none}");

    // ---- a print to the system print window --------------------------------------
    let item = |key: &str, copies: i64, price: Value| json!({ "kind": "product", "productKey": key, "copies": copies, "priceCents": price });
    let (status, key, pdf) = shop
        .print_job(json!({
            "templateKey": PRICE_TAG,
            "items": [item(&cable_key, 3, json!(150_000))],
            "action": "print", "method": "system", "printerName": null, "dpi": 203,
        }))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pdf));
    assert_eq!(rendered_payload(&pdf)["labels"][0]["copies"], 3);
    let print_key = key.expect("the job key comes back in a header");
    assert!(print_key.starts_with("lpj_"));
    let job = shop.get(&format!("/api/labels/jobs/{print_key}")).await;
    assert_eq!(job["outcome"], "rendered");
    assert_eq!(job["action"], "print");
    assert_eq!(job["method"], "system");
    assert_eq!(job["dpi"], 203);
    assert!(job["printerName"].is_null());
    assert_eq!(job["templateKey"], PRICE_TAG);
    assert_eq!(job["templateName"], "Price tag 50 x 30 mm");
    assert_eq!(job["totalLabels"], 3);
    assert!(!job["printedByName"].as_str().unwrap().is_empty());
    let printed = &job["items"][0];
    assert_eq!(printed["productKey"], cable_key.as_str());
    assert_eq!(
        printed["productName"],
        format!("Cable {}", shop.tag).as_str()
    );
    assert_eq!(printed["priceCents"], 150_000);
    assert_eq!(printed["copies"], 3);
    assert!(printed["serialNumber"].is_null());

    // The computer reports back once; a second report or a bad one is refused.
    let uri = format!("/api/labels/jobs/{print_key}");
    for (body, expected) in [
        (
            json!({ "outcome": "rendered", "error": null }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "outcome": "sent", "error": "why" }),
            StatusCode::BAD_REQUEST,
        ),
        (json!({ "outcome": "sent", "error": null }), StatusCode::OK),
        (
            json!({ "outcome": "failed", "error": "jam" }),
            StatusCode::CONFLICT,
        ),
    ] {
        let (status, response) = send(
            &shop.router,
            "PATCH",
            &uri,
            Some(body.clone()),
            &shop.manager,
        )
        .await;
        assert_eq!(status, expected, "{body}: {response}");
    }
    assert_eq!(shop.get(&uri).await["outcome"], "sent");
    let (status, body) = send(
        &shop.router,
        "PATCH",
        "/api/labels/jobs/lpj_missing",
        Some(json!({ "outcome": "sent", "error": null })),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "LABEL_JOB_NOT_FOUND");

    // ---- a failed direct print does not count as labelled ------------------------
    let (status, key, _) = shop
        .print_job(json!({
            "templateKey": PRICE_TAG,
            "items": [item(&charger_key, 2, json!(90_000))],
            "action": "print", "method": "direct", "printerName": " Zebra ZD220 ", "dpi": 300,
        }))
        .await;
    assert_eq!(status, StatusCode::OK);
    let failed_uri = format!("/api/labels/jobs/{}", key.unwrap());
    let (status, failed) = send(
        &shop.router,
        "PATCH",
        &failed_uri,
        Some(json!({ "outcome": "failed", "error": "Printer is offline" })),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{failed}");
    assert_eq!(failed["data"]["outcome"], "failed");
    assert_eq!(failed["data"]["error"], "Printer is offline");
    assert_eq!(failed["data"]["printerName"], "Zebra ZD220");

    // ---- serial labels: one group per unit, recorded with the serial number ----
    let (status, _, _) = shop
        .print_job(json!({
            "templateKey": PRICE_TAG,
            "items": serials.iter().map(|n| json!({ "kind": "serial", "serialNumber": n, "copies": 2 })).collect::<Vec<_>>(),
            "action": "download", "method": null, "printerName": null, "dpi": null,
        }))
        .await;
    assert_eq!(status, StatusCode::OK);
    let downloads = shop
        .get(&format!(
            "/api/labels/jobs?action=download&productKey={phone_key}"
        ))
        .await;
    assert_eq!(downloads["pagination"]["total"], 1, "{downloads}");
    let download = &downloads["items"][0];
    assert_eq!(download["outcome"], "sent");
    assert!(download["method"].is_null() && download["dpi"].is_null());
    assert_eq!(download["totalLabels"], 4);
    assert_eq!(download["items"][1]["serialNumber"], serials[1].as_str());
    assert_eq!(download["items"][1]["productKey"], phone_key.as_str());
    // A download was handed over at once: there is nothing to report.
    let (status, body) = send(
        &shop.router,
        "PATCH",
        &format!("/api/labels/jobs/{}", download["key"].as_str().unwrap()),
        Some(json!({ "outcome": "sent", "error": null })),
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "LABEL_JOB_ALREADY_REPORTED");

    // ---- incomplete print methods and unprintable items are not recorded --------
    for (body, why) in [
        (
            json!({ "action": "download", "method": "system", "printerName": null, "dpi": null }),
            "download with a method",
        ),
        (
            json!({ "action": "print", "method": "system", "printerName": null, "dpi": null }),
            "print without dpi",
        ),
        (
            json!({ "action": "print", "method": "direct", "printerName": null, "dpi": 203 }),
            "direct print without a printer",
        ),
        (
            json!({ "action": "print", "method": null, "printerName": null, "dpi": 203 }),
            "print without a method",
        ),
    ] {
        let mut request = body;
        request["templateKey"] = json!(PRICE_TAG);
        request["items"] = json!([item(&cable_key, 1, json!(150_000))]);
        let (status, key, _) = shop.print_job(request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}");
        assert!(key.is_none());
    }
    let (status, key, _) = shop
        .print_job(json!({
            "templateKey": PRICE_TAG,
            "items": [item(&cable_key, 1, Value::Null)],
            "action": "download", "method": null, "printerName": null, "dpi": null,
        }))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(key.is_none());

    // ---- history listing ------------------------------------------------------------
    let all = shop.get("/api/labels/jobs?limit=100").await;
    let mine: Vec<&Value> = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|job| {
            job["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["productName"].as_str().unwrap().ends_with(&shop.tag))
        })
        .collect();
    assert_eq!(mine.len(), 3, "{all}");
    // Newest first: the download, then the failed print, then the first print.
    assert_eq!(mine[0]["action"], "download");
    assert_eq!(mine[2]["key"], print_key.as_str());
    let page = shop.get("/api/labels/jobs?limit=1&page=2").await;
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["pagination"]["limit"], 1);
    let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    let none = shop
        .get(&format!(
            "/api/labels/jobs?from={}",
            future.replace('+', "%2B")
        ))
        .await;
    assert_eq!(none["pagination"]["total"], 0);
    let (status, body) = send(
        &shop.router,
        "GET",
        "/api/labels/jobs?from=yesterday",
        None,
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // ---- coverage after printing -------------------------------------------------------
    let coverage = shop.get(&search).await;
    let cable_row = row_of(&coverage, &cable_key);
    assert_eq!(cable_row["status"], "labelled", "{coverage}");
    assert_eq!(cable_row["lastLabelledCopies"], 3);
    assert_eq!(cable_row["receivedSinceLabel"], 0);
    assert_eq!(row_of(&coverage, &charger_key)["status"], "never");
    let phone_row = row_of(&coverage, &phone_key);
    assert_eq!(phone_row["status"], "labelled");
    assert_eq!(phone_row["lastLabelledCopies"], 4);

    // New stock after the last label means its labels are needed.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    shop.purchase(&cable_key, 4, 160_000, None).await;
    let coverage = shop.get(&search).await;
    let cable_row = row_of(&coverage, &cable_key);
    assert_eq!(cable_row["status"], "needed", "{coverage}");
    assert_eq!(cable_row["receivedSinceLabel"], 4);
    assert_eq!(coverage["counts"]["never"], 1);
    assert_eq!(coverage["counts"]["needed"], 1);
    assert_eq!(coverage["counts"]["labelled"], 1);
    // Never labelled first, then labels needed, then labelled.
    let order: Vec<&str> = coverage["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["product"]["key"].as_str().unwrap())
        .collect();
    assert_eq!(&order[..3], [&charger_key, &cable_key, &phone_key]);

    let needed = shop.get(&format!("{search}&status=needed")).await;
    assert_eq!(needed["pagination"]["total"], 1);
    assert_eq!(needed["items"][0]["product"]["key"], cable_key.as_str());
    // The row carries the whole product, price tiers included, ready to print.
    assert_eq!(
        needed["items"][0]["product"]["priceTiers"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // A past job is printed again with today's details: only the named products.
    let picked_uri = format!("/api/labels/coverage?productKeys={cable_key},{phone_key}");
    let picked = shop.get(&picked_uri).await;
    assert_eq!(picked["pagination"]["total"], 2, "{picked}");
    // What the shop paid stays Admin-only.
    for row in picked["items"].as_array().unwrap() {
        assert!(
            row["product"]["priceTiers"][0]
                .get("minUnitCostCents")
                .is_none()
        );
    }
    let (status, admin_view) = send(&shop.router, "GET", &picked_uri, None, &shop.admin).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        admin_view["data"]["items"][0]["product"]["priceTiers"][0]["minUnitCostCents"].is_number()
    );
    let too_many: Vec<String> = (0..201).map(|i| format!("prod_{i}")).collect();
    let (status, body) = send(
        &shop.router,
        "GET",
        &format!("/api/labels/coverage?productKeys={}", too_many.join(",")),
        None,
        &shop.manager,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn label_jobs_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    jobs_scenario(&app).await;
}

#[tokio::test]
async fn label_jobs_on_mongo() {
    let app = common::spawn_app_isolated().await;
    jobs_scenario(&app).await;
    app.db.drop().await.ok();
}

#[tokio::test]
async fn each_shop_has_its_own_print_history() {
    let app = common::spawn_app_multi_tenant().await;
    let admin_of = |tenant: &str| {
        common::mint_token_for_tenant(
            &app.config,
            Some(Role::Admin),
            roles::default_permissions(Role::Admin),
            Some(tenant),
        )
    };
    let (shop_a, shop_b) = (admin_of("tnt_jobs_a"), admin_of("tnt_jobs_b"));

    let (status, category) = send(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "Jobs Cat", "icon": "box", "color": "blue" })),
        &shop_a,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{category}");
    let category_key = category["data"]["key"].as_str().unwrap().to_string();
    let (_, sub) = send(
        &app.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Items" })),
        &shop_a,
    )
    .await;
    let (status, product) = send(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Shop A cable",
            "categoryKey": category_key,
            "subcategoryKey": sub["data"]["subcategories"][0]["key"],
            "minStockThreshold": 1,
            "autoGenerateBarcode": true,
        })),
        &shop_a,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{product}");
    let (status, design) = send(
        &app.router,
        "POST",
        "/api/labels/templates",
        Some(design("Barcode only", json!([barcode(46.0)]))),
        &shop_a,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{design}");

    let (status, pdf) = send_raw(
        &app.router,
        "POST",
        "/api/labels/jobs",
        Some(json!({
            "templateKey": design["data"]["key"],
            "items": [{ "kind": "product", "productKey": product["data"]["key"], "copies": 1, "priceCents": null }],
            "action": "download", "method": null, "printerName": null, "dpi": null,
        })),
        &shop_a,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pdf));
    let (_, a) = send(&app.router, "GET", "/api/labels/jobs", None, &shop_a).await;
    let (_, b) = send(&app.router, "GET", "/api/labels/jobs", None, &shop_b).await;
    assert_eq!(a["data"]["pagination"]["total"], 1, "{a}");
    assert_eq!(
        b["data"]["pagination"]["total"], 0,
        "shop B must not see shop A's prints"
    );
    let key = a["data"]["items"][0]["key"].as_str().unwrap();
    let (status, _) = send(
        &app.router,
        "GET",
        &format!("/api/labels/jobs/{key}"),
        None,
        &shop_b,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    app.db.drop().await.ok();
}
