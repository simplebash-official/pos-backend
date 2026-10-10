mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::json;
use tower::ServiceExt;

use simplebash_pos_backend::domain::users::Role;

async fn send_request(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().method(method).uri(uri);

    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }

    let request = if let Some(json_body) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&json_body).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let json: serde_json::Value = if body_bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null)
    };

    (status, json)
}

#[tokio::test]
async fn test_shop_profile_initial_defaults_are_empty_in_sqlite() {
    let app = common::spawn_app_sqlite().await;
    let token = common::mint_token(&app.config, Some(Role::Admin), &[]);

    let (status, res) =
        send_request(&app.router, "GET", "/api/settings/shop-profile", Some(&token), None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["success"], true);
    let data = &res["data"];
    assert_eq!(data["legalName"], "");
    assert_eq!(data["tradingName"], "");
    assert_eq!(data["addressLines"], json!([]));
    assert_eq!(data["primaryPhone"], "");
    assert_eq!(data["secondaryPhone"], "");
    assert_eq!(data["email"], "");
    assert_eq!(data["website"], "");
    assert_eq!(data["version"], 1);
}

#[tokio::test]
async fn test_shop_profile_update_and_persistence() {
    let app = common::spawn_app_sqlite().await;
    let admin_token = common::mint_token(&app.config, Some(Role::Admin), &[]);
    let cashier_token = common::mint_token(&app.config, Some(Role::Staff), &[]);

    // 1. Update shop profile as Admin
    let update_body = json!({
        "legalName": "Acme Retail Ltd",
        "tradingName": "Shop 55",
        "addressLines": ["No. 42, Galle Road", "Colombo 03"],
        "primaryPhone": "0771234567",
        "secondaryPhone": "0112345678",
        "email": "contact@shop55.lk",
        "website": "https://shop55.lk"
    });

    let (status, update_res) = send_request(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(&admin_token),
        Some(update_body),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(update_res["success"], true);
    let updated = &update_res["data"];
    assert_eq!(updated["legalName"], "Acme Retail Ltd");
    assert_eq!(updated["tradingName"], "Shop 55");
    assert_eq!(
        updated["addressLines"],
        json!(["No. 42, Galle Road", "Colombo 03"])
    );
    assert_eq!(updated["primaryPhone"], "0771234567");
    assert_eq!(updated["secondaryPhone"], "0112345678");
    assert_eq!(updated["email"], "contact@shop55.lk");
    assert_eq!(updated["website"], "https://shop55.lk");

    // 2. Fetch as Cashier — verify persistence
    let (get_status, get_res) = send_request(
        &app.router,
        "GET",
        "/api/settings/shop-profile",
        Some(&cashier_token),
        None,
    )
    .await;

    assert_eq!(get_status, StatusCode::OK);
    let data = &get_res["data"];
    assert_eq!(data["legalName"], "Acme Retail Ltd");
    assert_eq!(data["tradingName"], "Shop 55");
    assert_eq!(
        data["addressLines"],
        json!(["No. 42, Galle Road", "Colombo 03"])
    );
    assert_eq!(data["primaryPhone"], "0771234567");
    assert_eq!(data["secondaryPhone"], "0112345678");
    assert_eq!(data["email"], "contact@shop55.lk");
    assert_eq!(data["website"], "https://shop55.lk");

    // 3. Cashier cannot update shop profile
    let (fail_status, _) = send_request(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(&cashier_token),
        Some(json!({ "tradingName": "Hacked Shop" })),
    )
    .await;
    assert_eq!(fail_status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_shop_profile_empty_trading_name_rejected() {
    let app = common::spawn_app_sqlite().await;
    let admin_token = common::mint_token(&app.config, Some(Role::Admin), &[]);

    let (status, res) = send_request(
        &app.router,
        "PUT",
        "/api/settings/shop-profile",
        Some(&admin_token),
        Some(json!({ "tradingName": "   " })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(res["success"], false);
}
