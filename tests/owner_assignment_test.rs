//! The shop owner (the Admin) as the person who does a repair or print job, on SQLite: the
//! Admin makes their own employee profile once (`POST /employees/me`), is then a normal choice
//! for assignment, and never takes a commission (a split on their job is refused, and moving a
//! job to them clears the split it had).

mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::domain::users::{CreateUserRequest, Role};
use simplebash_pos_backend::modules::users;
use tower::ServiceExt;

async fn send(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .expect("the request is well formed");
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

fn token(app: &common::TestApp, role: Role) -> String {
    common::mint_token(&app.config, Some(role), roles::default_permissions(role))
}

fn repair(phone_suffix: &str, assignment: Value) -> Value {
    json!({
        "customer": { "customerName": "Kasun Silva", "customerPhone": format!("077{phone_suffix}") },
        "deviceModel": "iPhone 14",
        "issueDescription": "Cracked screen",
        "items": [],
        "assignment": assignment,
    })
}

#[tokio::test]
async fn the_owner_makes_their_own_profile_once_and_takes_no_commission() {
    let app = common::spawn_app_sqlite().await;
    users::service::create_user(
        &app.db_handle,
        CreateUserRequest {
            name: "Mayura Owner".to_string(),
            username: roles::ADMIN_USERNAME.to_string(),
            password: "Password123!".to_string(),
            role: Role::Admin,
            employee_key: None,
            email: None,
        },
    )
    .await
    .expect("the admin account is created");
    let admin = token(&app, Role::Admin);

    // Only the Admin may make it.
    let (status, _) = send(
        &app,
        "POST",
        "/api/employees/me",
        Some(json!({ "phone": "0771234567" })),
        &token(&app, Role::Manager),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // The phone is checked like any employee's.
    let (status, _) = send(
        &app,
        "POST",
        "/api/employees/me",
        Some(json!({ "phone": "" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, body) = send(
        &app,
        "POST",
        "/api/employees/me",
        Some(json!({ "phone": "0771234567" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let owner = body["data"].clone();
    assert_eq!(owner["role"], "owner");
    assert_eq!(owner["name"], "Mayura Owner");
    assert_eq!(owner["login"]["username"], roles::ADMIN_USERNAME);
    let owner_key = owner["key"].as_str().expect("the key is a string");

    // A second profile is refused.
    let (status, body) = send(
        &app,
        "POST",
        "/api/employees/me",
        Some(json!({ "phone": "0771234567" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "OWNER_PROFILE_EXISTS");

    // The profile cannot be deleted while it is the Admin's login.
    let (status, body) = send(
        &app,
        "DELETE",
        &format!("/api/employees/{}", owner["id"].as_str().expect("an id")),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "EMPLOYEE_HAS_LOGIN");

    // A job for the owner with a commission is refused.
    let (status, body) = send(
        &app,
        "POST",
        "/api/repairs",
        Some(repair(
            "1000001",
            json!({ "assignedEmployeeId": owner_key, "splitType": "percentage", "splitValue": 20.0 }),
        )),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "OWNER_HAS_NO_COMMISSION");

    // Without one it is saved, under the owner's name.
    let (status, body) = send(
        &app,
        "POST",
        "/api/repairs",
        Some(repair(
            "1000002",
            json!({ "assignedEmployeeId": owner_key }),
        )),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["assignedEmployeeName"], "Mayura Owner");
    assert!(body["data"]["splitType"].is_null());

    // Moving a job that had a commission to the owner clears it; a split sent later is refused.
    let (status, technician) = send(
        &app,
        "POST",
        "/api/employees",
        Some(json!({
            "name": "Kamal", "phone": "0779999999", "role": "technician",
            "defaultSplitType": "percentage", "defaultSplitValue": 20.0,
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{technician}");
    let (status, job) = send(
        &app,
        "POST",
        "/api/repairs",
        Some(repair(
            "1000003",
            json!({
                "assignedEmployeeId": technician["data"]["key"],
                "splitType": "percentage", "splitValue": 20.0,
            }),
        )),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{job}");
    assert_eq!(job["data"]["splitType"], "percentage");
    let job_id = job["data"]["id"].as_str().expect("the job has an id");

    let (status, body) = send(
        &app,
        "PATCH",
        &format!("/api/repairs/{job_id}"),
        Some(json!({ "assignment": { "assignedEmployeeId": owner_key } })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["assignedEmployeeName"], "Mayura Owner");
    assert!(body["data"]["splitType"].is_null(), "{body}");
    assert!(body["data"]["splitValue"].is_null(), "{body}");

    let (status, body) = send(
        &app,
        "PATCH",
        &format!("/api/repairs/{job_id}"),
        Some(json!({ "assignment": { "splitType": "fixed", "splitValue": 500.0 } })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "OWNER_HAS_NO_COMMISSION");

    // Print jobs follow the same rule.
    let print_job = |assignment: Value| {
        json!({
            "customer": { "customerName": "Nadeesha Perera", "customerPhone": "0711234567" },
            "jobType": "t-shirt",
            "quantity": 12,
            "estimatedCostCents": 240000,
            "assignment": assignment,
        })
    };
    let (status, body) = send(
        &app,
        "POST",
        "/api/print-jobs",
        Some(print_job(
            json!({ "assignedEmployeeId": owner_key, "splitType": "fixed", "splitValue": 100.0 }),
        )),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "OWNER_HAS_NO_COMMISSION");
    let (status, body) = send(
        &app,
        "POST",
        "/api/print-jobs",
        Some(print_job(json!({ "assignedEmployeeId": owner_key }))),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["assignedEmployeeName"], "Mayura Owner");
}
