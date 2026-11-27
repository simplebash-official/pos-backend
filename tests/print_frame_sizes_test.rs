//! The shop's frame size list and the frames a photo and framing job asks for, end to end on
//! SQLite and on a throwaway Atlas database: adding, hiding and showing sizes, the same frame typed
//! another way, every limit, who may read or change the list, and jobs that record wanted frames
//! without them changing any money.

mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use simplebash_pos_backend::clients::sqlite;
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::logging::SqlLogging;
use simplebash_pos_backend::domain::users::Role;
use sqlx::Executor;
use tower::ServiceExt;

const URI: &str = "/api/print-frame-sizes";
const ALL: &str = "/api/print-frame-sizes?includeHidden=true";
const JOBS: &str = "/api/print-jobs";

struct Tokens {
    admin: String,
    manager: String,
    staff: String,
}

impl Tokens {
    fn of(app: &common::TestApp) -> Tokens {
        let token =
            |role| common::mint_token(&app.config, Some(role), roles::default_permissions(role));
        Tokens {
            admin: token(Role::Admin),
            manager: token(Role::Manager),
            staff: token(Role::Staff),
        }
    }
}

async fn send(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = request
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
    let json = serde_json::from_slice(&bytes).expect("the body is JSON");
    (status, json)
}

/// Sends a request that must succeed, and returns its `data`.
async fn ok(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> Value {
    let (status, answer) = send(app, method, uri, body, Some(token)).await;
    assert!(status.is_success(), "{method} {uri}: {status} {answer}");
    answer["data"].clone()
}

/// Sends a request that must be refused with this status and error code; returns the message.
async fn refused(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
    expected: (StatusCode, &str),
) -> String {
    let (status, answer) = send(app, method, uri, body, Some(token)).await;
    assert_eq!(status, expected.0, "{method} {uri}: {answer}");
    assert_eq!(answer["code"], expected.1, "{method} {uri}: {answer}");
    answer["message"]
        .as_str()
        .expect("a refusal explains itself")
        .to_string()
}

const INVALID: (StatusCode, &str) = (StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
const LOCKED: (StatusCode, &str) = (StatusCode::CONFLICT, "PRINT_JOB_ITEMS_LOCKED");

fn text(value: &Value) -> String {
    value.as_str().expect("the value is a string").to_string()
}

fn size(size: &str, width: i64) -> Value {
    json!({ "size": size, "widthHundredthsInch": width })
}

/// `(size, width, active)` of every row, in the order sent.
fn rows(list: &Value) -> Vec<(String, i64, bool)> {
    list.as_array()
        .expect("the list is an array")
        .iter()
        .map(|row| {
            (
                text(&row["size"]),
                row["widthHundredthsInch"].as_i64().expect("a width"),
                row["active"].as_bool().expect("a flag"),
            )
        })
        .collect()
}

async fn nothing_is_preloaded(app: &common::TestApp, tokens: &Tokens) {
    let list = ok(app, "GET", ALL, None, &tokens.admin).await;
    assert_eq!(rows(&list), []);
}

/// Adds `8x10` at two widths and returns the keys, narrow first.
async fn add_sizes(app: &common::TestApp, tokens: &Tokens) -> (String, String) {
    let narrow = ok(app, "POST", URI, Some(size("  8x10 ", 75)), &tokens.manager).await;
    let key = text(&narrow["key"]);
    assert!(key.starts_with("pfs_"), "{narrow}");
    assert_eq!(narrow["size"], "8x10", "the size is trimmed");
    assert_eq!(narrow["active"], true, "a new size is offered");
    assert_eq!(narrow["version"], 1);
    assert!(narrow.get("priceCents").is_none(), "a size has no price");
    let wide = ok(app, "POST", URI, Some(size("8x10", 100)), &tokens.admin).await;
    (key, text(&wide["key"]))
}

async fn the_same_frame_is_refused(app: &common::TestApp, tokens: &Tokens) {
    for typed in ["8x10", "8 X 10", "8×10", " 8 x 10 "] {
        let message = refused(
            app,
            "POST",
            URI,
            Some(size(typed, 75)),
            &tokens.admin,
            (StatusCode::CONFLICT, "PRINT_FRAME_SIZE_EXISTS"),
        )
        .await;
        assert_eq!(
            message, "8x10 at 0.75 inch is already in the list.",
            "{typed}"
        );
    }
}

async fn limits(app: &common::TestApp, tokens: &Tokens) {
    let bad = [
        size("", 75),
        size("   ", 75),
        size("123456789012345678901", 75),
        size("5x7", 0),
        size("5x7", -75),
        size("5x7", 10_001),
    ];
    for body in bad {
        refused(app, "POST", URI, Some(body), &tokens.admin, INVALID).await;
    }
    // The edges themselves are accepted.
    for body in [size("12345678901234567890", 1), size("24x36", 10_000)] {
        ok(app, "POST", URI, Some(body), &tokens.admin).await;
    }
}

async fn hide_and_show(app: &common::TestApp, tokens: &Tokens, key: &str) {
    let one = format!("{URI}/{key}");
    let hidden = ok(
        app,
        "PUT",
        &one,
        Some(json!({ "active": false })),
        &tokens.manager,
    )
    .await;
    assert_eq!(hidden["active"], false);
    assert_eq!(hidden["version"], 2);
    assert_eq!(hidden["size"], "8x10");
    assert_eq!(hidden["widthHundredthsInch"], 75);

    let offered = ok(app, "GET", URI, None, &tokens.staff).await;
    assert!(
        !rows(&offered)
            .iter()
            .any(|row| row.0 == "8x10" && row.1 == 75),
        "{offered}"
    );
    let all = ok(app, "GET", ALL, None, &tokens.manager).await;
    assert!(rows(&all).contains(&("8x10".to_string(), 75, false)));

    let message = refused(
        app,
        "POST",
        URI,
        Some(size("8x10", 75)),
        &tokens.admin,
        (StatusCode::CONFLICT, "PRINT_FRAME_SIZE_EXISTS"),
    )
    .await;
    assert_eq!(
        message,
        "8x10 at 0.75 inch is already saved but hidden. Turn on Show hidden and show it again."
    );

    let shown = ok(
        app,
        "PUT",
        &one,
        Some(json!({ "active": true })),
        &tokens.admin,
    )
    .await;
    assert_eq!(shown["version"], 3);
    let offered = ok(app, "GET", URI, None, &tokens.staff).await;
    assert!(rows(&offered).contains(&("8x10".to_string(), 75, true)));

    refused(
        app,
        "PUT",
        &format!("{URI}/pfs_000000000000"),
        Some(json!({ "active": true })),
        &tokens.admin,
        (StatusCode::NOT_FOUND, "PRINT_FRAME_SIZE_NOT_FOUND"),
    )
    .await;
}

async fn permissions(app: &common::TestApp, tokens: &Tokens, key: &str) {
    let denied = (StatusCode::FORBIDDEN, "PERMISSION_DENIED");
    refused(
        app,
        "POST",
        URI,
        Some(size("6x8", 75)),
        &tokens.staff,
        denied,
    )
    .await;
    refused(
        app,
        "PUT",
        &format!("{URI}/{key}"),
        Some(json!({ "active": false })),
        &tokens.staff,
        denied,
    )
    .await;
    refused(app, "GET", ALL, None, &tokens.staff, denied).await;
    let nobody = common::mint_token(&app.config, Some(Role::Staff), &[]);
    refused(app, "GET", URI, None, &nobody, denied).await;
    for (method, uri) in [("GET", URI), ("POST", URI)] {
        let (status, _) = send(app, method, uri, Some(size("6x8", 75)), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

fn wanted(key: &str, quantity: i64) -> Value {
    json!({ "frameSizeKey": key, "quantity": quantity })
}

fn framing_job(customer: &str, frames: Value) -> Value {
    json!({
        "customer": { "customerKey": customer }, "jobType": "photo-framing", "quantity": 1,
        "items": [], "frames": frames,
    })
}

/// A framing job records the frames with the list's own size and width, and costs nothing.
async fn a_job_records_wanted_frames(
    app: &common::TestApp,
    tokens: &Tokens,
    customer: &str,
    (narrow, wide): (&str, &str),
) -> String {
    let created = ok(
        app,
        "POST",
        JOBS,
        Some(framing_job(
            customer,
            json!([wanted(wide, 2), wanted(narrow, 1)]),
        )),
        &tokens.staff,
    )
    .await;
    assert_eq!(
        created["frames"],
        json!([
            { "frameSizeKey": wide, "size": "8x10", "widthHundredthsInch": 100, "quantity": 2 },
            { "frameSizeKey": narrow, "size": "8x10", "widthHundredthsInch": 75, "quantity": 1 },
        ])
    );
    assert_eq!(created["priceCents"], 0, "frames price nothing");
    assert_eq!(created["items"], json!([]));
    let id = text(&created["key"]);
    let read = ok(app, "GET", &format!("{JOBS}/{id}"), None, &tokens.staff).await;
    assert_eq!(read["frames"], created["frames"]);

    // A job that sends no frames lists none.
    let plain = ok(
        app,
        "POST",
        JOBS,
        Some(json!({
            "customer": { "customerKey": customer }, "jobType": "mug", "quantity": 1, "items": [],
        })),
        &tokens.staff,
    )
    .await;
    assert_eq!(plain["frames"], json!([]));
    id
}

async fn wrong_frames_are_refused(
    app: &common::TestApp,
    tokens: &Tokens,
    customer: &str,
    key: &str,
) {
    let too_many: Vec<Value> = (0..51).map(|_| wanted(key, 1)).collect();
    for frames in [
        json!([wanted(key, 0)]),
        json!([wanted(key, 1_000_001)]),
        json!([wanted(key, 1), wanted(key, 1)]),
        Value::Array(too_many),
    ] {
        refused(
            app,
            "POST",
            JOBS,
            Some(framing_job(customer, frames)),
            &tokens.staff,
            INVALID,
        )
        .await;
    }
    refused(
        app,
        "POST",
        JOBS,
        Some(framing_job(
            customer,
            json!([wanted("pfs_000000000000", 1)]),
        )),
        &tokens.staff,
        (StatusCode::NOT_FOUND, "PRINT_FRAME_SIZE_NOT_FOUND"),
    )
    .await;
    let mut mug = framing_job(customer, json!([wanted(key, 1)]));
    mug["jobType"] = json!("mug");
    refused(app, "POST", JOBS, Some(mug), &tokens.staff, INVALID).await;
}

/// A hidden size stays on the jobs that have it, with a new quantity if wanted, but cannot be
/// added to a job; changing the type needs the frames removed; other edits keep them.
async fn editing_a_jobs_frames(
    app: &common::TestApp,
    tokens: &Tokens,
    customer: &str,
    job: &str,
    (narrow, wide): (&str, &str),
) {
    let one = format!("{JOBS}/{job}");
    ok(
        app,
        "PUT",
        &format!("{URI}/{wide}"),
        Some(json!({ "active": false })),
        &tokens.admin,
    )
    .await;
    let kept = ok(
        app,
        "PATCH",
        &one,
        Some(json!({ "frames": [wanted(wide, 5)] })),
        &tokens.staff,
    )
    .await;
    assert_eq!(kept["frames"][0]["quantity"], 5);
    assert_eq!(kept["frames"][0]["widthHundredthsInch"], 100);
    assert_eq!(kept["frames"].as_array().map(Vec::len), Some(1));
    let message = refused(
        app,
        "POST",
        JOBS,
        Some(framing_job(customer, json!([wanted(wide, 1)]))),
        &tokens.staff,
        INVALID,
    )
    .await;
    assert_eq!(
        message,
        "8x10 at 1 inch is hidden, so it cannot be added to a job."
    );

    let noted = ok(
        app,
        "PATCH",
        &one,
        Some(json!({ "note": "Black frame" })),
        &tokens.staff,
    )
    .await;
    assert_eq!(
        noted["frames"], kept["frames"],
        "other edits keep the frames"
    );

    refused(
        app,
        "PATCH",
        &one,
        Some(json!({ "jobType": "mug" })),
        &tokens.staff,
        INVALID,
    )
    .await;
    let retyped = ok(
        app,
        "PATCH",
        &one,
        Some(json!({ "jobType": "mug", "frames": [] })),
        &tokens.staff,
    )
    .await;
    assert_eq!(retyped["frames"], json!([]));
    let back = ok(
        app,
        "PATCH",
        &one,
        Some(json!({ "jobType": "photo-framing", "frames": [wanted(narrow, 3)] })),
        &tokens.staff,
    )
    .await;
    assert_eq!(back["frames"][0]["quantity"], 3);
}

async fn a_handed_back_job_keeps_its_frames(
    app: &common::TestApp,
    tokens: &Tokens,
    job: &str,
    narrow: &str,
) {
    let one = format!("{JOBS}/{job}");
    let delivered = ok(
        app,
        "PATCH",
        &one,
        Some(json!({
            "status": "delivered",
            "items": [{
                "description": "Framing", "quantity": 1,
                "source": { "type": "service", "unitPriceCents": 50_000 },
                "warranty": { "coverage": { "kind": "none" } },
            }],
        })),
        &tokens.staff,
    )
    .await;
    assert_eq!(delivered["frames"][0]["quantity"], 3);
    assert_eq!(delivered["priceCents"], 50_000, "only the item is charged");
    refused(
        app,
        "PATCH",
        &one,
        Some(json!({ "frames": [wanted(narrow, 4)] })),
        &tokens.staff,
        LOCKED,
    )
    .await;
}

async fn scenario(app: &common::TestApp) {
    let tokens = Tokens::of(app);
    nothing_is_preloaded(app, &tokens).await;
    let (narrow, wide) = add_sizes(app, &tokens).await;
    the_same_frame_is_refused(app, &tokens).await;
    limits(app, &tokens).await;
    hide_and_show(app, &tokens, &narrow).await;
    permissions(app, &tokens, &narrow).await;

    let customer = common::register_customer(app, "Nimal Perera", "0771234567").await;
    let job = a_job_records_wanted_frames(app, &tokens, &customer, (&narrow, &wide)).await;
    wrong_frames_are_refused(app, &tokens, &customer, &narrow).await;
    editing_a_jobs_frames(app, &tokens, &customer, &job, (&narrow, &wide)).await;
    a_handed_back_job_keeps_its_frames(app, &tokens, &job, &narrow).await;
}

#[tokio::test]
async fn print_frame_sizes_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn print_frame_sizes_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo()
        .drop()
        .await
        .expect("the test database is dropped");
}

#[tokio::test]
async fn an_old_sqlite_database_gains_the_wanted_frames_column() {
    let path = std::env::temp_dir().join(format!("pos_frames_{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("the database opens");
    // The table as it was before a job could list frames.
    pool.execute("ALTER TABLE print_jobs DROP COLUMN frames")
        .await
        .expect("the old shape is set up");

    sqlite::init_db(&pool).await.expect("the migration runs");
    sqlite::init_db(&pool).await.expect("a second start runs");

    let declared: String = sqlx::query_scalar(
        "SELECT type FROM pragma_table_info('print_jobs') WHERE name = 'frames'",
    )
    .fetch_one(&pool)
    .await
    .expect("the column exists");
    // Sync reads a column as JSON by its declared type.
    assert_eq!(declared, "JSON");
    pool.close().await;
    std::fs::remove_file(&path).expect("the temporary database is removed");
}
