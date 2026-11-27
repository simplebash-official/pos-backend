//! What a job's own work costs the shop: the optional cost on repair labour, on print service
//! lines and on both lists of ready-made services, through to the job's cost and the invoice, on
//! SQLite and MongoDB, and the SQLite column and backup upgrades.

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

async fn send(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
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
    let json = serde_json::from_slice(&bytes).expect("the body is JSON");
    (status, json)
}

/// Sends a request that must succeed and returns its `data`.
async fn ok(app: &common::TestApp, method: &str, uri: &str, body: Option<Value>) -> Value {
    let (status, answer) = send(app, method, uri, body).await;
    assert!(status.is_success(), "{method} {uri}: {status} {answer}");
    answer["data"].clone()
}

/// Sends a request that must be refused as invalid.
async fn refused(app: &common::TestApp, method: &str, uri: &str, body: Value) {
    let (status, answer) = send(app, method, uri, Some(body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{method} {uri}: {answer}");
}

fn text(value: &Value) -> String {
    value.as_str().expect("the value is a string").to_string()
}

fn no_cover() -> Value {
    json!({ "coverage": { "kind": "none" } })
}

/// A ready-made service keeps, changes and forgets its usual cost.
async fn ready_made_service(app: &common::TestApp, uri: &str, price: &str, cost: &str) {
    let mut body = json!({ "name": "Design work", "warranty": no_cover(), "active": true });
    body[price] = json!(2_500);
    body[cost] = json!(-1);
    refused(app, "POST", uri, body.clone()).await;

    body[cost] = json!(900);
    let created = ok(app, "POST", uri, Some(body.clone())).await;
    assert_eq!(created[cost], 900, "{created}");
    let one = format!("{uri}/{}", text(&created["key"]));
    let listed = ok(app, "GET", uri, None).await;
    assert_eq!(listed[0][cost], 900, "{listed}");

    // A typed zero is a recorded cost, not a missing one.
    body[cost] = json!(0);
    let zeroed = ok(app, "PUT", &one, Some(body.clone())).await;
    assert_eq!(zeroed[cost], 0, "{zeroed}");

    body.as_object_mut().expect("an object").remove(cost);
    let cleared = ok(app, "PUT", &one, Some(body)).await;
    assert!(cleared.get(cost).is_none(), "{cleared}");
    let listed = ok(app, "GET", uri, None).await;
    assert!(listed[0].get(cost).is_none(), "{listed}");
}

/// Bills one ticket and returns the invoice line's recorded unit cost.
async fn billed_unit_cost(app: &common::TestApp, customer: &str, kind: &str, key: &str) -> Value {
    let invoice = ok(
        app,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier Kamal" },
            "customer": { "customerKey": customer },
            "items": [{
                "quantity": 1, "discountCents": 0, "sourceType": kind,
                "sourceTicketKey": key, "handover": "later",
            }],
            "payment": {
                "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 10_000_000,
            },
            "shopProfileSnapshot": { "tradingName": "Cost Test Shop" },
        })),
    )
    .await;
    invoice["invoice"]["items"][0]["unitCostCents"].clone()
}

async fn repair_labour_cost(app: &common::TestApp, customer: &str) {
    let fee = |cost: Option<i64>| {
        let mut item = json!({ "description": "Board repair", "laborCents": 5_000,
                               "warranty": no_cover() });
        if let Some(cost) = cost {
            item["laborCostCents"] = json!(cost);
        }
        item
    };
    let job = |items: Value| {
        json!({
            "customer": { "customerKey": customer }, "deviceModel": "Galaxy A55",
            "issueDescription": "No power", "items": items,
        })
    };
    refused(app, "POST", "/api/repairs", job(json!([fee(Some(-1))]))).await;

    let with_part = json!({
        "description": "Replace port", "laborCents": 0, "warranty": no_cover(),
        "part": { "source": "outside", "name": "Port", "priceCents": 800, "costCents": 300 },
    });
    let created = ok(
        app,
        "POST",
        "/api/repairs",
        Some(job(json!([fee(Some(1_500)), with_part]))),
    )
    .await;
    assert_eq!(created["priceCents"], 5_800, "{created}");
    assert_eq!(created["costCents"], 1_800);
    assert_eq!(created["items"][0]["laborCostCents"], 1_500);
    assert!(created["items"][1].get("laborCostCents").is_none());
    let key = text(&created["key"]);
    let uri = format!("/api/repairs/{key}");
    let read = ok(app, "GET", &uri, None).await;
    assert_eq!(read["items"], created["items"]);

    // An edit sends the whole list: leaving the cost out forgets it.
    let mut cleared = fee(None);
    cleared["key"] = created["items"][0]["key"].clone();
    let edited = ok(app, "PATCH", &uri, Some(json!({ "items": [cleared] }))).await;
    assert_eq!(edited["costCents"], 0, "{edited}");
    assert!(edited["items"][0].get("laborCostCents").is_none());

    let mut costed = fee(Some(2_000));
    costed["key"] = created["items"][0]["key"].clone();
    let edited = ok(app, "PATCH", &uri, Some(json!({ "items": [costed] }))).await;
    assert_eq!(edited["costCents"], 2_000);
    assert_eq!(billed_unit_cost(app, customer, "repair", &key).await, 2_000);
}

async fn print_service_cost(app: &common::TestApp, customer: &str) {
    let line = |cost: Option<i64>| {
        let mut item = json!({ "description": "Design", "quantity": 3, "warranty": no_cover(),
                               "source": { "type": "service", "unitPriceCents": 1_000 } });
        if let Some(cost) = cost {
            item["source"]["unitCostCents"] = json!(cost);
        }
        item
    };
    let job = |items: Value| {
        json!({
            "customer": { "customerKey": customer }, "jobType": "mug", "quantity": 3,
            "items": items,
        })
    };
    refused(app, "POST", "/api/print-jobs", job(json!([line(Some(-1))]))).await;
    // Three units at this cost no longer fit in an amount.
    refused(
        app,
        "POST",
        "/api/print-jobs",
        job(json!([line(Some(i64::MAX))])),
    )
    .await;

    let created = ok(
        app,
        "POST",
        "/api/print-jobs",
        Some(job(json!([line(Some(400)), line(None)]))),
    )
    .await;
    assert_eq!(created["priceCents"], 6_000, "{created}");
    assert_eq!(created["costCents"], 1_200);
    assert_eq!(created["items"][0]["source"]["unitCostCents"], 400);
    assert_eq!(created["items"][0]["costCents"], 1_200);
    assert!(created["items"][1]["source"].get("unitCostCents").is_none());
    assert_eq!(created["items"][1]["costCents"], 0);
    let key = text(&created["key"]);
    let uri = format!("/api/print-jobs/{key}");
    let read = ok(app, "GET", &uri, None).await;
    assert_eq!(read["items"], created["items"]);

    let mut cleared = line(None);
    cleared["key"] = created["items"][0]["key"].clone();
    let edited = ok(app, "PATCH", &uri, Some(json!({ "items": [cleared] }))).await;
    assert_eq!(edited["costCents"], 0, "{edited}");
    assert!(edited["items"][0]["source"].get("unitCostCents").is_none());

    let mut costed = line(Some(250));
    costed["key"] = created["items"][0]["key"].clone();
    let edited = ok(app, "PATCH", &uri, Some(json!({ "items": [costed] }))).await;
    assert_eq!(edited["costCents"], 750);
    assert_eq!(billed_unit_cost(app, customer, "print", &key).await, 750);
}

async fn scenario(app: &common::TestApp) {
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("the admin is seeded");
    let customer = common::register_customer(app, "Nimal Perera", "0771234567").await;
    ready_made_service(app, "/api/repair-services", "laborCents", "laborCostCents").await;
    ready_made_service(
        app,
        "/api/print-services",
        "unitPriceCents",
        "unitCostCents",
    )
    .await;
    repair_labour_cost(app, &customer).await;
    print_service_cost(app, &customer).await;
}

#[tokio::test]
async fn work_costs_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn work_costs_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.mongo()
        .drop()
        .await
        .expect("the test database is dropped");
}

#[tokio::test]
async fn an_old_sqlite_database_gains_the_ready_made_service_cost_columns() {
    let path = std::env::temp_dir().join(format!("pos_work_cost_{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("the database opens");
    // The two tables as they were before a service could remember a cost.
    for statement in [
        "ALTER TABLE repair_service_templates DROP COLUMN labor_cost_cents",
        "ALTER TABLE print_service_templates DROP COLUMN unit_cost_cents",
        "INSERT INTO repair_service_templates (key, id, name, labor_cents, warranty, active, \
         version, created_at, updated_at) VALUES ('rst_old', '65f1a1a1a1a1a1a1a1a1a1f1', \
         'Battery swap', 2500, '{\"coverage\":{\"kind\":\"none\"}}', 1, 1, \
         '2026-03-10T05:00:00+00:00', '2026-03-10T05:00:00+00:00')",
    ] {
        pool.execute(statement)
            .await
            .expect("the old shape is set up");
    }

    sqlite::init_db(&pool).await.expect("the migration runs");
    sqlite::init_db(&pool).await.expect("a second start runs");

    for (table, column) in [
        ("repair_service_templates", "labor_cost_cents"),
        ("print_service_templates", "unit_cost_cents"),
    ] {
        let columns: Vec<String> =
            sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .fetch_all(&pool)
                .await
                .expect("columns read");
        assert!(columns.iter().any(|c| c == column), "{table}.{column}");
    }
    let cost: Option<i64> = sqlx::query_scalar(
        "SELECT labor_cost_cents FROM repair_service_templates WHERE key = 'rst_old'",
    )
    .fetch_one(&pool)
    .await
    .expect("the old service is read");
    assert_eq!(cost, None);
    drop(pool);
    common::remove_sqlite_files(&path);
}

#[tokio::test]
async fn a_backup_without_the_cost_columns_restores_services_with_no_cost() {
    let app = common::spawn_app_sqlite().await;
    let mut backup = ok(&app, "POST", "/api/backup/export", Some(json!({}))).await;
    backup["tables"]["print_service_templates"] = json!([{
        "key": "pst_old", "id": "65f1a1a1a1a1a1a1a1a1a1f2", "name": "Design",
        "unit_price_cents": 1_000, "warranty": no_cover().to_string(), "active": 1, "version": 1,
        "created_at": "2026-03-12T05:00:00+00:00", "updated_at": "2026-03-12T05:00:00+00:00",
    }]);
    ok(
        &app,
        "POST",
        "/api/backup/import",
        Some(json!({ "backup": backup })),
    )
    .await;
    let listed = ok(&app, "GET", "/api/print-services", None).await;
    assert_eq!(listed[0]["unitPriceCents"], 1_000, "{listed}");
    assert!(listed[0].get("unitCostCents").is_none(), "{listed}");
}
