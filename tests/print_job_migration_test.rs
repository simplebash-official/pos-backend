//! Print jobs with one quoted price and one material cost become jobs with lines, keeping their
//! price and cost: a live SQLite database at startup and a MongoDB database holding several
//! shops.

mod common;

use mongodb::bson::{Document, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::clients::db::Db;
use simplebash_pos_backend::clients::sqlite;
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::logging::SqlLogging;
use simplebash_pos_backend::core::tenancy::TENANT_FIELD;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use simplebash_pos_backend::modules::print_jobs::migration::migrate_items_mongo;
use sqlx::Executor;

const OLD_TABLE: &str = "CREATE TABLE print_jobs (key TEXT PRIMARY KEY, id TEXT NOT NULL, \
    ticket_number TEXT NOT NULL UNIQUE, customer_key TEXT NOT NULL, customer_name TEXT NOT NULL, \
    customer_phone TEXT NOT NULL, job_type TEXT NOT NULL, quantity INTEGER NOT NULL DEFAULT 1, \
    promised_ready_at TEXT, status TEXT NOT NULL, estimated_cost_cents INTEGER NOT NULL, \
    material_cost_cents INTEGER, assigned_employee_id TEXT, assigned_employee_name TEXT, \
    split_type TEXT, split_value REAL, billed_invoice_key TEXT, billed_invoice_number TEXT, \
    version INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, \
    deleted_at TEXT, updated_by_device TEXT)";

fn old_row(key: &str, number: &str, status: &str, amounts: &str) -> String {
    format!(
        "INSERT INTO print_jobs (key, id, ticket_number, customer_key, customer_name, \
         customer_phone, job_type, quantity, status, estimated_cost_cents, material_cost_cents, \
         version, created_at, updated_at) VALUES ('{key}', '65f1a1a1a1a1a1a1a2{number}', \
         'PRN-{number}', 'cus_aa', 'Kamal Perera', '0712223334', 'mug', 12, '{status}', \
         {amounts}, 7, '2026-03-12T05:00:00+00:00', '2026-03-13T05:00:00+00:00')"
    )
}

#[tokio::test]
async fn an_old_sqlite_database_moves_to_print_job_items() {
    let path = std::env::temp_dir().join(format!("pos_prn_items_{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("the database opens");
    for statement in [
        "DROP TABLE print_jobs".to_string(),
        OLD_TABLE.to_string(),
        old_row("prn_priced", "000001", "delivered", "2500000, 1300000"),
        // Saved with the status that never existed, too: both moves run.
        old_row("prn_quote", "000002", "in_progress", "50000, NULL"),
        old_row("prn_empty", "000003", "received", "0, NULL"),
    ] {
        pool.execute(statement.as_str())
            .await
            .expect("the old shape is set up");
    }

    sqlite::init_db(&pool).await.expect("the migration runs");

    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('print_jobs')")
            .fetch_all(&pool)
            .await
            .expect("columns read");
    for gone in ["estimated_cost_cents", "material_cost_cents"] {
        assert!(!columns.iter().any(|c| c == gone), "{gone} is dropped");
    }
    let db = Db::Sqlite(pool.clone());
    let job = |key: &'static str| {
        let db = db.clone();
        async move {
            simplebash_pos_backend::modules::print_jobs::service::get_print_job(&db, key)
                .await
                .expect("the job loads")
        }
    };
    let priced = job("prn_priced").await;
    assert_eq!(
        (priced.price_cents, priced.cost_cents),
        (2_500_000, 1_300_000)
    );
    assert_eq!(priced.version, 7, "a migration is not an edit");
    assert!(priced.note.is_none());
    let items = serde_json::to_value(&priced.items).expect("the lines serialize");
    assert_eq!(items[0]["key"], "pit_mig_prn_priced");
    assert_eq!(items[0]["description"], "Mug printing (12 units)");
    assert_eq!(items[0]["unitPriceCents"], 2_500_000);
    assert_eq!(items[1]["key"], "pit_mig_mat_prn_priced");
    assert_eq!(items[1]["costCents"], 1_300_000);
    assert_eq!(items[1]["unitPriceCents"], 0);
    let quote = job("prn_quote").await;
    assert_eq!(quote.status, "in_repair");
    assert_eq!((quote.items.len(), quote.price_cents), (1, 50_000));
    let empty = job("prn_empty").await;
    assert!(empty.items.is_empty());

    // A second start changes nothing.
    sqlite::init_db(&pool).await.expect("the second start runs");
    assert_eq!(job("prn_priced").await.items.len(), 2);
    drop(pool);
    std::fs::remove_file(&path).ok();
}

/// Sends one request as an admin and returns the reply's `data`, which must be a success.
async fn admin_call(app: &common::TestApp, method: &str, uri: &str, body: Option<Value>) -> Value {
    use axum::body::Body;
    use axum::http::Request;
    use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
    use tower::ServiceExt;

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
    let answer: Value = serde_json::from_slice(&bytes).expect("the body is JSON");
    assert!(status.is_success(), "{method} {uri}: {status} {answer}");
    answer["data"].clone()
}

#[tokio::test]
async fn a_backup_with_old_print_jobs_restores_them_with_items() {
    let app = common::spawn_app_sqlite().await;
    let mut backup = admin_call(&app, "POST", "/api/backup/export", Some(json!({}))).await;
    backup["tables"]["print_jobs"] = json!([{
        "key": "prn_backup", "id": "65f1a1a1a1a1a1a1a2000009", "ticket_number": "PRN-000009",
        "customer_key": "cus_aa", "customer_name": "Kamal Perera",
        "customer_phone": "0712223334", "job_type": "handbill", "quantity": 500,
        "status": "received", "estimated_cost_cents": 750_000, "material_cost_cents": 200_000,
        "version": 1, "created_at": "2026-03-12T05:00:00+00:00",
        "updated_at": "2026-03-13T05:00:00+00:00",
    }]);
    admin_call(
        &app,
        "POST",
        "/api/backup/import",
        Some(json!({ "backup": backup })),
    )
    .await;
    let job = admin_call(&app, "GET", "/api/print-jobs/prn_backup", None).await;
    assert_eq!(job["priceCents"], 750_000);
    assert_eq!(job["costCents"], 200_000);
    assert_eq!(
        job["items"][0]["description"],
        "Handbill printing (500 units)"
    );
}

async fn raw_job(app: &common::TestApp, key: &str, shop: &str, estimated: i64) {
    app.mongo()
        .collection::<Document>("print_jobs")
        .insert_one(doc! {
            "key": key, TENANT_FIELD: shop, "job_type": "banner", "quantity": 3_i32,
            "estimated_cost_cents": estimated, "material_cost_cents": 400_i32,
        })
        .await
        .expect("the print job is stored");
}

async fn stored(app: &common::TestApp, key: &str) -> Value {
    let document = app
        .mongo()
        .collection::<Document>("print_jobs")
        .find_one(doc! { "key": key })
        .await
        .expect("the print job is read")
        .expect("the print job exists");
    serde_json::to_value(document).expect("a document serializes")
}

#[tokio::test]
async fn a_mongo_database_moves_print_jobs_in_every_shop_to_items() {
    let app = common::spawn_app_isolated().await;
    raw_job(&app, "prn_a", "shop_a", 900).await;
    raw_job(&app, "prn_b", "shop_b", 0).await;

    // A dry run counts and writes nothing.
    let report = migrate_items_mongo(app.mongo(), Mode::DryRun)
        .await
        .expect("the dry run works");
    assert_eq!((report.print_jobs, report.with_items), (2, 2));
    assert!(stored(&app, "prn_a").await.get("items").is_none());

    let report = migrate_items_mongo(app.mongo(), Mode::Apply)
        .await
        .expect("the migration works");
    assert_eq!(report.print_jobs, 2);
    let job = stored(&app, "prn_a").await;
    assert!(job.get("estimated_cost_cents").is_none(), "{job}");
    assert_eq!(job["price_cents"], 900, "{job}");
    assert_eq!(job["cost_cents"], 400, "{job}");
    assert_eq!(job["items"][0]["key"], "pit_mig_prn_a");

    // A second run finds nothing to do.
    let report = migrate_items_mongo(app.mongo(), Mode::Apply)
        .await
        .expect("the second run works");
    assert_eq!(report.print_jobs, 0);
    app.mongo().drop().await.ok();
}
