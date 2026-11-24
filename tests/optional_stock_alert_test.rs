// A product's low-stock alert level is optional: `null` means no alert. Creating without a
// level, setting, changing and removing it, and every list, filter and count that depends on it.
// On SQLite and on a throwaway Atlas DB.

mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::middleware::auth::Claims;
use simplebash_pos_backend::domain::users::Role;
use tower::ServiceExt;

fn admin_token(app: &common::TestApp) -> String {
    let claims = Claims {
        sub: "usr_admin".to_string(),
        exp: 9_999_999_999,
        role: Some(Role::Admin),
        permissions: roles::default_permissions(Role::Admin)
            .iter()
            .map(|p| p.to_string())
            .collect(),
        tid: None,
        scope: None,
        did: None,
        name: Some("usr_admin".to_string()),
        pwv: None,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(app.config.jwt_secret.as_bytes()),
    )
    .expect("the token encodes")
}

async fn send(
    app: &common::TestApp,
    token: &str,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    let request = match body {
        Some(value) => builder
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(value.to_string()))
            .expect("the request is well formed"),
        None => builder
            .body(Body::empty())
            .expect("the request is well formed"),
    };
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body reads");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("the body is JSON")
    };
    (status, json)
}

fn text(value: &Value) -> String {
    value.as_str().expect("the value is a string").to_string()
}

fn names(body: &Value) -> Vec<String> {
    body["data"]["items"]
        .as_array()
        .expect("the items are a list")
        .iter()
        .map(|item| text(&item["name"]))
        .collect()
}

/// The names on the low-stock list.
async fn low_names(app: &common::TestApp, admin: &str) -> Vec<String> {
    let (status, body) = send(app, admin, "GET", "/api/inventory/products/low-stock", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    names(&body)
}

/// The low-stock count on the inventory header.
async fn low_count(app: &common::TestApp, admin: &str) -> Value {
    let (_, stats) = send(app, admin, "GET", "/api/inventory/stats", None).await;
    stats["data"]["lowStockAlerts"].clone()
}

async fn put(app: &common::TestApp, admin: &str, id: &str, body: Value) -> (StatusCode, Value) {
    send(
        app,
        admin,
        "PUT",
        &format!("/api/inventory/products/{id}"),
        Some(body),
    )
    .await
}

/// Everything a scenario step needs to name one product.
struct Item {
    id: String,
}

async fn scenario(app: &common::TestApp) {
    let admin = admin_token(app);
    let (status, category) = send(
        app,
        &admin,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": "Alerts", "icon": "Box", "color": "blue", "subcategories": ["Parts"],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{category}");
    let category_key = text(&category["data"]["key"]);
    let subcategory_key = text(&category["data"]["subcategories"][0]["key"]);

    let create = |name: &str, level: Option<Value>| {
        let mut body = json!({
            "name": name,
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 2, "unitCostCents": 300, "sellingPriceCents": 700 },
        });
        if let Some(level) = level {
            body["minStockThreshold"] = level;
        }
        body
    };

    // ---- Creating: a level, no field at all, an explicit null, a negative level ----
    let mut items = Vec::new();
    for (name, level) in [
        ("With level", Some(json!(5))),
        ("Field left out", None),
        ("Explicit null", Some(Value::Null)),
    ] {
        let (status, created) = send(
            app,
            &admin,
            "POST",
            "/api/inventory/products",
            Some(create(name, level.clone())),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        match level {
            Some(Value::Number(n)) => assert_eq!(created["data"]["minStockThreshold"], json!(n)),
            _ => assert!(
                created["data"]["minStockThreshold"].is_null(),
                "no level was given, so there is none: {created}"
            ),
        }
        let id = text(&created["data"]["id"]);
        let (_, read) = send(
            app,
            &admin,
            "GET",
            &format!("/api/inventory/products/{id}"),
            None,
        )
        .await;
        assert_eq!(
            read["data"]["minStockThreshold"], created["data"]["minStockThreshold"],
            "what is read back is what was saved: {read}"
        );
        items.push(Item { id });
    }
    let (status, refused) = send(
        app,
        &admin,
        "POST",
        "/api/inventory/products",
        Some(create("Negative", Some(json!(-1)))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");

    // ---- Reading: only the product with a level, at or below it, is low ----
    assert_eq!(low_names(app, &admin).await, ["With level"]);
    let (_, filtered) = send(
        app,
        &admin,
        "GET",
        &format!("/api/inventory/products?categoryKey={category_key}&lowStock=true&limit=50"),
        None,
    )
    .await;
    assert_eq!(names(&filtered), ["With level"], "{filtered}");
    assert_eq!(low_count(app, &admin).await, json!(1));

    // ---- Updating: null removes the alert, omitting leaves it, a number sets it ----
    let with_level = &items[0].id;
    let (status, cleared) = put(
        app,
        &admin,
        with_level,
        json!({ "minStockThreshold": null }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert!(cleared["data"]["minStockThreshold"].is_null(), "{cleared}");
    assert!(low_names(app, &admin).await.is_empty());
    assert_eq!(low_count(app, &admin).await, json!(0));

    let (_, set) = put(app, &admin, with_level, json!({ "minStockThreshold": 4 })).await;
    assert_eq!(set["data"]["minStockThreshold"], json!(4), "{set}");
    let (_, renamed) = put(app, &admin, with_level, json!({ "name": "Renamed" })).await;
    assert_eq!(
        renamed["data"]["minStockThreshold"],
        json!(4),
        "an update that does not mention the level leaves it: {renamed}"
    );
    assert_eq!(low_names(app, &admin).await, ["Renamed"]);

    let field_left_out = &items[1].id;
    let (_, given) = put(
        app,
        &admin,
        field_left_out,
        json!({ "minStockThreshold": 3 }),
    )
    .await;
    assert_eq!(given["data"]["minStockThreshold"], json!(3), "{given}");
    let mut now_low = low_names(app, &admin).await;
    now_low.sort();
    assert_eq!(now_low, ["Field left out", "Renamed"]);

    let (status, refused) = put(
        app,
        &admin,
        field_left_out,
        json!({ "minStockThreshold": -1 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    let (_, unchanged) = send(
        app,
        &admin,
        "GET",
        &format!("/api/inventory/products/{field_left_out}"),
        None,
    )
    .await;
    assert_eq!(
        unchanged["data"]["minStockThreshold"],
        json!(3),
        "{unchanged}"
    );

    // A level of 0 is a real level: the product is only low once it is empty.
    let (_, zero) = put(
        app,
        &admin,
        field_left_out,
        json!({ "minStockThreshold": 0 }),
    )
    .await;
    assert_eq!(zero["data"]["minStockThreshold"], json!(0), "{zero}");
    assert_eq!(low_names(app, &admin).await, ["Renamed"]);
}

/// An import row with no alert column, or a blank one, gives a product with no alert; a number
/// is kept, and a negative one fails only its own row.
async fn import_scenario(app: &common::TestApp) {
    let admin = admin_token(app);
    let (status, body) = send(
        app,
        &admin,
        "POST",
        "/api/imports",
        Some(json!({
            "target": "inventory",
            "fileName": "stock.csv",
            "fileType": "csv",
            "rows": [
                { "name": "Column absent", "category": "Imports", "subcategory": "Parts",
                  "sellingPrice": 500 },
                { "name": "Blank cell", "category": "Imports", "subcategory": "Parts",
                  "sellingPrice": 500, "minStockThreshold": "" },
                { "name": "Has a level", "category": "Imports", "subcategory": "Parts",
                  "sellingPrice": 500, "minStockThreshold": 4 },
                { "name": "Level zero", "category": "Imports", "subcategory": "Parts",
                  "sellingPrice": 500, "minStockThreshold": 0 },
                { "name": "Negative level", "category": "Imports", "subcategory": "Parts",
                  "sellingPrice": 500, "minStockThreshold": -2 },
            ],
            "options": { "autoCreateCategories": true },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["successfulRows"], 4, "{body}");
    assert_eq!(body["data"]["failedRows"], 1, "{body}");

    let (_, listed) = send(app, &admin, "GET", "/api/inventory/products?limit=50", None).await;
    let level_of = |name: &str| -> Value {
        listed["data"]["items"]
            .as_array()
            .expect("the items are a list")
            .iter()
            .find(|item| item["name"] == name)
            .unwrap_or_else(|| panic!("{name} was imported: {listed}"))["minStockThreshold"]
            .clone()
    };
    assert!(level_of("Column absent").is_null());
    assert!(level_of("Blank cell").is_null());
    assert_eq!(level_of("Has a level"), json!(4));
    assert_eq!(level_of("Level zero"), json!(0));
    assert!(
        !listed["data"]["items"]
            .as_array()
            .expect("the items are a list")
            .iter()
            .any(|item| item["name"] == "Negative level"),
        "the row with a negative level was refused"
    );
}

#[tokio::test]
async fn importing_without_an_alert_level_leaves_it_empty_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    import_scenario(&app).await;
}

#[tokio::test]
async fn importing_without_an_alert_level_leaves_it_empty_on_mongo() {
    let app = common::spawn_app_isolated().await;
    import_scenario(&app).await;
    app.mongo().drop().await.ok();
}

#[tokio::test]
async fn the_alert_level_is_optional_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn the_alert_level_is_optional_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}
