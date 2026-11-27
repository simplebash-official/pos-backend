//! The slip a customer gets for the repair and print jobs they leave with the shop, on SQLite and
//! on a throwaway Atlas database: what it says (the mock document-server echoes the payload into
//! its fake PDF), one job or several on one slip, the papers, the permissions, the refusals, and
//! that a changed ticket or changed shop terms print afresh.

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

    let slip =
        |jobs: &str, paper: &str| format!("/api/job-receipts/document?jobs={jobs}&paper={paper}");

    // What the slip of one repair says.
    let (status, pdf) = send(app, "GET", &slip(&repair_key, "a5"), None, &staff).await;
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
    assert!(pdf.contains("\"estimateTotalCents\":null"), "{pdf}");
    assert!(pdf.contains("\"promisedReadyOn\":\"27 Nov 2026\""), "{pdf}");
    assert!(pdf.contains("\"termsText\":\"Keep this slip.\""), "{pdf}");
    assert!(pdf.contains("\"uncollectedAfterDays\":30"), "{pdf}");
    assert!(pdf.contains("\"shopTradingName\":\"Shop 55\""), "{pdf}");

    // Every paper prints; anything else, or none, is refused.
    for paper in ["thermal-58", "thermal-80"] {
        let (status, pdf) = send(app, "GET", &slip(&repair_key, paper), None, &staff).await;
        assert_eq!(status, StatusCode::OK, "{paper}: {pdf}");
        assert!(pdf.contains(&format!("\"paper\":\"{paper}\"")), "{pdf}");
    }
    for refused in [
        slip(&repair_key, "a4"),
        format!("/api/job-receipts/document?jobs={repair_key}"),
        "/api/job-receipts/document?paper=a5".to_string(),
    ] {
        let (status, body) = send(app, "GET", &refused, None, &staff).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}: {body}");
    }

    // Unknown ticket, no token rights.
    for missing in ["rep_missing", "prn_missing"] {
        let (status, _) = send(app, "GET", &slip(missing, "a5"), None, &staff).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
    }
    let (status, _) = send(app, "GET", &slip(&repair_key, "a5"), None, &no_write).await;
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
    let (_, pdf) = send(app, "GET", &slip(&repair_key, "a5"), None, &staff).await;
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
    let (_, pdf) = send(app, "GET", &slip(&repair_key, "a5"), None, &staff).await;
    assert!(pdf.contains("\"termsText\":\"New terms.\""), "{pdf}");
    assert!(pdf.contains("\"uncollectedAfterDays\":null"), "{pdf}");

    // A print job.
    let print_job = |customer: String| {
        json!({
            "customer": { "customerKey": customer },
            "jobType": "mug",
            "quantity": 20,
            "items": [{ "description": "Printing", "quantity": 1,
                    "source": { "type": "service", "unitPriceCents": 800000 },
                    "warranty": { "coverage": { "kind": "none" } } }]
        })
    };
    let job = send_json(
        app,
        "POST",
        "/api/print-jobs",
        Some(print_job(customer.clone())),
        &staff,
    )
    .await;
    let job_key = job["data"]["key"].as_str().expect("a key").to_string();
    let job_number = job["data"]["ticketNumber"].as_str().expect("a number");
    let (status, pdf) = send(app, "GET", &slip(&job_key, "thermal-80"), None, &staff).await;
    assert_eq!(status, StatusCode::OK, "{pdf}");
    assert!(pdf.contains("\"jobKind\":\"print\""), "{pdf}");
    assert!(
        pdf.contains(&format!("\"ticketNumber\":\"{job_number}\"")),
        "{pdf}"
    );
    assert!(pdf.contains("\"detail\":\"20 × Mug\""), "{pdf}");
    assert!(pdf.contains("\"estimateCents\":800000"), "{pdf}");
    assert!(pdf.contains("\"itemLines\":[\"Printing x 1\"]"), "{pdf}");

    // Both on one slip, in the order asked, with the total.
    let both = format!("{job_key},{repair_key}");
    let (status, pdf) = send(app, "GET", &slip(&both, "a5"), None, &staff).await;
    assert_eq!(status, StatusCode::OK, "{pdf}");
    let print_at = pdf.find(job_number).expect("the print job is listed");
    let repair_at = pdf.find(repair_number).expect("the repair is listed");
    assert!(print_at < repair_at, "{pdf}");
    assert!(pdf.contains("\"estimateTotalCents\":2650000"), "{pdf}");

    // Each kind on the slip needs its own permission.
    let repairs_only = token(app, Role::Staff, &["repairs:write"]);
    let prints_only = token(app, Role::Staff, &["print_jobs:write"]);
    for (token, jobs, expected) in [
        (&repairs_only, repair_key.as_str(), StatusCode::OK),
        (&repairs_only, job_key.as_str(), StatusCode::FORBIDDEN),
        (&repairs_only, both.as_str(), StatusCode::FORBIDDEN),
        (&prints_only, job_key.as_str(), StatusCode::OK),
        (&prints_only, repair_key.as_str(), StatusCode::FORBIDDEN),
        (&prints_only, both.as_str(), StatusCode::FORBIDDEN),
    ] {
        let (status, body) = send(app, "GET", &slip(jobs, "a5"), None, token).await;
        assert_eq!(status, expected, "{jobs}: {body}");
    }

    // One slip is one customer's.
    let other = common::register_customer(app, "Kamal Silva", "0779999999").await;
    let theirs = send_json(
        app,
        "POST",
        "/api/print-jobs",
        Some(print_job(other)),
        &staff,
    )
    .await;
    let theirs = theirs["data"]["key"].as_str().expect("a key");
    let mixed = format!("{repair_key},{theirs}");
    let (status, body) = send(app, "GET", &slip(&mixed, "a5"), None, &staff).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("JOB_RECEIPT_CUSTOMERS_DIFFER"), "{body}");

    // Up to 20 jobs, each once, and nothing that is not a job.
    let mut keys = vec![repair_key.clone(), job_key.clone()];
    for _ in 0..19 {
        let more = send_json(
            app,
            "POST",
            "/api/print-jobs",
            Some(print_job(customer.clone())),
            &staff,
        )
        .await;
        keys.push(more["data"]["key"].as_str().expect("a key").to_string());
    }
    let (status, body) = send(app, "GET", &slip(&keys[..20].join(","), "a5"), None, &staff).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for refused in [
        keys.join(","),
        String::new(),
        format!("{repair_key},{repair_key}"),
        format!("{repair_key},"),
        "inv_1".to_string(),
    ] {
        let (status, body) = send(app, "GET", &slip(&refused, "a5"), None, &staff).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "`{refused}`: {body}");
        assert!(
            body.contains("JOB_RECEIPT_JOBS_INVALID"),
            "`{refused}`: {body}"
        );
    }

    // The old per-job addresses are gone.
    let (status, _) = send(
        app,
        "GET",
        &format!("/api/repairs/{repair_key}/documents/job-receipt?paper=a5"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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
