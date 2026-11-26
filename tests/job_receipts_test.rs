//! The receipt a customer gets when the shop receives a repair or a print job, on SQLite and on
//! a throwaway Atlas database: what it says (the mock document-server echoes the payload into
//! its fake PDF), the papers, the permissions, and that a changed ticket or changed shop terms
//! print afresh.

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
) -> (StatusCode, String) {
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
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn send_json(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> Value {
    let (status, text) = send(app, method, uri, body, token).await;
    assert_eq!(status, StatusCode::OK, "{method} {uri}: {text}");
    serde_json::from_str(&text).expect("the body is JSON")
}

fn token(app: &common::TestApp, role: Role, permissions: &[&str]) -> String {
    common::mint_token(&app.config, Some(role), permissions)
}

async fn scenario(app: &common::TestApp) {
    let admin = token(app, Role::Admin, roles::default_permissions(Role::Admin));
    let staff = token(app, Role::Staff, roles::default_permissions(Role::Staff));
    let no_write = token(app, Role::Staff, &[]);

    send_json(
        app,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({
            "tradingName": "Shop 55",
            "jobReceiptTermsText": "Keep this slip.",
            "uncollectedAfterDays": 30
        })),
        &admin,
    )
    .await;

    let customer = common::register_customer(app, "Nimal Perera", "0771234567").await;
    let repair = send_json(
        app,
        "POST",
        "/api/repairs",
        Some(json!({
            "customer": { "customerKey": customer },
            "deviceModel": "Samsung Galaxy A15",
            "serialNumber": "356789104512347",
            "issueDescription": "Screen cracked",
            "promisedReadyAt": "2026-11-27",
            "items": [{
                "description": "Replace display",
                "laborCents": 500000,
                "part": { "source": "outside", "name": "Display", "priceCents": 1350000, "costCents": 900000 },
                "warranty": { "coverage": { "kind": "none" } }
            }]
        })),
        &staff,
    )
    .await;
    let repair_key = repair["data"]["key"].as_str().expect("a key").to_string();
    let repair_number = repair["data"]["ticketNumber"].as_str().expect("a number");

    let receipt =
        |paper: &str| format!("/api/repairs/{repair_key}/documents/job-receipt?paper={paper}");

    // What the repair receipt says.
    let (status, pdf) = send(app, "GET", &receipt("a5"), None, &staff).await;
    assert_eq!(status, StatusCode::OK, "{pdf}");
    assert!(pdf.contains("\"paper\":\"a5\""), "{pdf}");
    assert!(
        pdf.contains(&format!("\"ticketNumber\":\"{repair_number}\"")),
        "{pdf}"
    );
    assert!(
        pdf.contains(&format!("\"qrData\":\"{repair_number}\"")),
        "{pdf}"
    );
    assert!(pdf.contains("\"jobKind\":\"repair\""), "{pdf}");
    assert!(pdf.contains("\"customerName\":\"Nimal Perera\""), "{pdf}");
    assert!(
        pdf.contains("\"serialNumber\":\"356789104512347\""),
        "{pdf}"
    );
    assert!(pdf.contains("\"itemLines\":[\"Replace display\"]"), "{pdf}");
    assert!(pdf.contains("\"estimateCents\":1850000"), "{pdf}");
    assert!(pdf.contains("\"promisedReadyOn\":\"27 Nov 2026\""), "{pdf}");
    assert!(pdf.contains("\"termsText\":\"Keep this slip.\""), "{pdf}");
    assert!(pdf.contains("\"uncollectedAfterDays\":30"), "{pdf}");
    assert!(pdf.contains("\"shopTradingName\":\"Shop 55\""), "{pdf}");

    // Every paper prints; anything else, or none, is refused.
    for paper in ["thermal-58", "thermal-80"] {
        let (status, pdf) = send(app, "GET", &receipt(paper), None, &staff).await;
        assert_eq!(status, StatusCode::OK, "{paper}: {pdf}");
        assert!(pdf.contains(&format!("\"paper\":\"{paper}\"")), "{pdf}");
    }
    let (status, _) = send(app, "GET", &receipt("a4"), None, &staff).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(
        app,
        "GET",
        &format!("/api/repairs/{repair_key}/documents/job-receipt"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Unknown ticket, no token rights.
    let (status, _) = send(
        app,
        "GET",
        "/api/repairs/rep_missing/documents/job-receipt?paper=a5",
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(app, "GET", &receipt("a5"), None, &no_write).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A changed ticket prints afresh.
    send_json(
        app,
        "PATCH",
        &format!("/api/repairs/{repair_key}"),
        Some(json!({ "deviceModel": "Samsung Galaxy A16" })),
        &staff,
    )
    .await;
    let (_, pdf) = send(app, "GET", &receipt("a5"), None, &staff).await;
    assert!(pdf.contains("\"title\":\"Samsung Galaxy A16\""), "{pdf}");

    // Changed shop terms print afresh too, and a removed warning disappears.
    send_json(
        app,
        "PUT",
        "/api/settings/shop-profile",
        Some(json!({ "jobReceiptTermsText": "New terms.", "uncollectedAfterDays": 0 })),
        &admin,
    )
    .await;
    let (_, pdf) = send(app, "GET", &receipt("a5"), None, &staff).await;
    assert!(pdf.contains("\"termsText\":\"New terms.\""), "{pdf}");
    assert!(pdf.contains("\"uncollectedAfterDays\":null"), "{pdf}");

    // A print job.
    let job = send_json(
        app,
        "POST",
        "/api/print-jobs",
        Some(json!({
            "customer": { "customerKey": customer },
            "jobType": "mug",
            "quantity": 20,
            "items": [{ "description": "Printing", "quantity": 1,
                    "source": { "type": "service", "unitPriceCents": 800000 },
                    "warranty": { "coverage": { "kind": "none" } } }]
        })),
        &staff,
    )
    .await;
    let job_key = job["data"]["key"].as_str().expect("a key").to_string();
    let job_number = job["data"]["ticketNumber"].as_str().expect("a number");
    let (status, pdf) = send(
        app,
        "GET",
        &format!("/api/print-jobs/{job_key}/documents/job-receipt?paper=thermal-80"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pdf}");
    assert!(pdf.contains("\"jobKind\":\"print\""), "{pdf}");
    assert!(
        pdf.contains(&format!("\"ticketNumber\":\"{job_number}\"")),
        "{pdf}"
    );
    assert!(pdf.contains("\"detail\":\"20 × Mug\""), "{pdf}");
    assert!(pdf.contains("\"estimateCents\":800000"), "{pdf}");
    assert!(pdf.contains("\"itemLines\":[\"Printing x 1\"]"), "{pdf}");
    let (status, _) = send(
        app,
        "GET",
        &format!("/api/print-jobs/{job_key}/documents/job-receipt?paper=a4"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(
        app,
        "GET",
        "/api/print-jobs/prn_missing/documents/job-receipt?paper=a5",
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        app,
        "GET",
        &format!("/api/print-jobs/{job_key}/documents/job-receipt?paper=a5"),
        None,
        &no_write,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn job_receipts_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn job_receipts_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo().drop().await.ok();
}
