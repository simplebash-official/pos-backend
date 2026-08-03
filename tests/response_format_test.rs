// No router, no Mongo — calls `ApiResponse`/`AppError` directly and
// asserts the JSON envelope shape. The fastest/narrowest of the three test
// tiers (see CLAUDE.md); use this style for anything about the
// response/error envelope itself, not endpoint behavior.

use axum::{http::StatusCode, response::IntoResponse};
use jana2u_pos_backend::core::{error::AppError, response::ApiResponse};
use serde_json::json;

#[tokio::test]
async fn test_success_response_format() {
    let response = ApiResponse::success(
        json!({"id": "123", "name": "Test Product"}),
        "Product retrieved successfully",
    )
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["id"], "123");
    assert_eq!(json["data"]["name"], "Test Product");
    assert_eq!(json["message"], "Product retrieved successfully");
}

#[tokio::test]
async fn test_error_response_format() {
    let err = AppError::custom(
        StatusCode::NOT_FOUND,
        "PRODUCT_NOT_FOUND",
        "Product not found",
    );
    let response = err.into_response();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["success"], false);
    assert_eq!(json["message"], "Product not found");
    assert_eq!(json["code"], "PRODUCT_NOT_FOUND");
    assert_eq!(json["statusCode"], 404);
}

#[tokio::test]
async fn test_standard_app_error_defaults() {
    let err = AppError::not_found("Item missing");
    let response = err.into_response();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["success"], false);
    assert_eq!(json["message"], "Item missing");
    assert_eq!(json["code"], "NOT_FOUND");
    assert_eq!(json["statusCode"], 404);
}
