//! One customer's jobs, on SQLite and on a throwaway Atlas database: the repair and print job
//! lists take a `customerKey`, answer only that customer's live jobs, page them, and refuse a
//! blank key.

mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::domain::users::Role;
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
        serde_json::from_slice(&bytes).unwrap_or_else(|err| {
            panic!(
                "{method} {uri} answered {status} with {:?}: {err}",
                String::from_utf8_lossy(&bytes)
            )
        })
    };
    (status, json)
}

async fn ok(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> Value {
    let (status, answer) = send(app, method, uri, body, token).await;
    assert!(status.is_success(), "{method} {uri}: {status} {answer}");
    answer["data"].clone()
}

fn text(value: &Value) -> String {
    value.as_str().expect("the value is a string").to_string()
}

/// The keys on one page of a list, sorted.
fn keys(page: &Value, list: &str) -> Vec<String> {
    let mut keys: Vec<String> = page[list]
        .as_array()
        .expect("a list")
        .iter()
        .map(|job| text(&job["key"]))
        .collect();
    keys.sort();
    keys
}

/// Creates three jobs for one customer and one for another, deletes one of the three, and checks
/// what the list answers for the first customer.
async fn check_list(
    app: &common::TestApp,
    token: &str,
    path: &str,
    list: &str,
    job_for: impl Fn(&str) -> Value,
    (customer, other): (&str, &str),
) {
    let mut mine = Vec::new();
    for _ in 0..3 {
        let job = ok(app, "POST", path, Some(job_for(customer)), token).await;
        mine.push(text(&job["key"]));
    }
    ok(app, "POST", path, Some(job_for(other)), token).await;
    let gone = mine.pop().expect("three jobs were made");
    ok(app, "DELETE", &format!("{path}/{gone}"), None, token).await;
    mine.sort();

    let all = ok(
        app,
        "GET",
        &format!("{path}?customerKey={customer}"),
        None,
        token,
    )
    .await;
    assert_eq!(keys(&all, list), mine, "{all}");
    assert_eq!(all["total"], 2, "{all}");

    let mut paged = Vec::new();
    for page in 1..=2 {
        let one = ok(
            app,
            "GET",
            &format!("{path}?customerKey={customer}&limit=1&page={page}"),
            None,
            token,
        )
        .await;
        assert_eq!(one["totalPages"], 2, "{one}");
        paged.extend(keys(&one, list));
    }
    paged.sort();
    assert_eq!(paged, mine);

    // Together with another filter, and for a customer with nothing.
    let none = ok(
        app,
        "GET",
        &format!("{path}?customerKey={customer}&status=delivered"),
        None,
        token,
    )
    .await;
    assert_eq!(none["total"], 0, "{none}");
    let nobody = ok(
        app,
        "GET",
        &format!("{path}?customerKey=cust_nobody"),
        None,
        token,
    )
    .await;
    assert_eq!(nobody["total"], 0, "{nobody}");

    // A blank key is a mistake, not "every job".
    for blank in ["", "%20%20"] {
        let (status, body) = send(
            app,
            "GET",
            &format!("{path}?customerKey={blank}"),
            None,
            token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
}

async fn scenario(app: &common::TestApp) {
    let token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let customer = common::register_customer(app, "Nimal Perera", "0771234567").await;
    let other = common::register_customer(app, "Kamal Silva", "0779999999").await;

    check_list(
        app,
        &token,
        "/api/repairs",
        "repairs",
        |key| {
            json!({ "customer": { "customerKey": key }, "deviceModel": "Galaxy A15",
                    "issueDescription": "Screen cracked", "items": [] })
        },
        (&customer, &other),
    )
    .await;
    check_list(
        app,
        &token,
        "/api/print-jobs",
        "printJobs",
        |key| {
            json!({ "customer": { "customerKey": key }, "jobType": "mug", "quantity": 2,
                    "items": [] })
        },
        (&customer, &other),
    )
    .await;
}

#[tokio::test]
async fn customer_jobs_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn customer_jobs_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}
