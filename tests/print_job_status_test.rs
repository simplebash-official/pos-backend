//! Print jobs saved as `in_progress`, a status that was never meant to exist, become
//! `in_repair` ("In production"): a live SQLite database at startup and a MongoDB database
//! holding several shops.

mod common;

use mongodb::bson::{Document, doc};
use simplebash_pos_backend::clients::sqlite;
use simplebash_pos_backend::core::logging::SqlLogging;
use simplebash_pos_backend::core::tenancy::TENANT_FIELD;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use simplebash_pos_backend::modules::print_jobs::migration::migrate_mongo;
use sqlx::Executor;

fn print_row(key: &str, ticket: &str, status: &str, deleted_at: &str) -> String {
    format!(
        "INSERT INTO print_jobs (key, id, ticket_number, customer_key, customer_name, \
         customer_phone, job_type, status, kind, items, price_cents, cost_cents, version, \
         created_at, updated_at, deleted_at) VALUES ('{key}', '65f1a1a1a1a1a1a1a2{ticket}', \
         'PRN-{ticket}', 'cus_aa', 'Kamal Perera', '0712223334', 'mug', '{status}', \
         '{{\"kind\":\"standard\"}}', '[]', 0, 0, 7, '2026-03-12T05:00:00+00:00', \
         '2026-03-13T05:00:00+00:00', {deleted_at})"
    )
}

async fn row(pool: &sqlx::SqlitePool, key: &str) -> (String, i64, String) {
    sqlx::query_as("SELECT status, version, updated_at FROM print_jobs WHERE key = ?")
        .bind(key)
        .fetch_one(pool)
        .await
        .expect("the print job is stored")
}

#[tokio::test]
async fn an_old_sqlite_database_moves_in_progress_print_jobs_to_in_repair() {
    let path = std::env::temp_dir().join(format!("pos_prn_status_{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("the database opens");

    for statement in [
        print_row("prn_old", "000001", "in_progress", "NULL"),
        // A soft-deleted job is moved too: it can be restored.
        print_row(
            "prn_gone",
            "000002",
            "in_progress",
            "'2026-03-14T05:00:00+00:00'",
        ),
        print_row("prn_new", "000003", "received", "NULL"),
        print_row("prn_ready", "000004", "ready", "NULL"),
    ] {
        pool.execute(statement.as_str())
            .await
            .expect("the old rows are stored");
    }

    sqlite::init_db(&pool).await.expect("the migration runs");

    let moved = (
        "in_repair".to_string(),
        7,
        "2026-03-13T05:00:00+00:00".to_string(),
    );
    assert_eq!(row(&pool, "prn_old").await, moved);
    assert_eq!(row(&pool, "prn_gone").await, moved);
    // Any other status is left alone, and so is everything but the status.
    assert_eq!(row(&pool, "prn_new").await.0, "received");
    assert_eq!(row(&pool, "prn_ready").await.0, "ready");

    // A second start changes nothing.
    sqlite::init_db(&pool).await.expect("the second start runs");
    assert_eq!(row(&pool, "prn_old").await, moved);
    drop(pool);
    std::fs::remove_file(&path).ok();
}

async fn raw_print_job(app: &common::TestApp, key: &str, shop: &str, status: &str) {
    app.mongo()
        .collection::<Document>("print_jobs")
        .insert_one(doc! { "key": key, TENANT_FIELD: shop, "status": status, "quantity": 3 })
        .await
        .expect("the print job is stored");
}

async fn status_of(app: &common::TestApp, key: &str) -> String {
    app.mongo()
        .collection::<Document>("print_jobs")
        .find_one(doc! { "key": key })
        .await
        .expect("the print job is read")
        .expect("the print job exists")
        .get_str("status")
        .expect("the status is text")
        .to_string()
}

#[tokio::test]
async fn a_mongo_database_moves_in_progress_print_jobs_in_every_shop() {
    let app = common::spawn_app_isolated().await;
    raw_print_job(&app, "prn_a", "shop_a", "in_progress").await;
    raw_print_job(&app, "prn_b", "shop_b", "in_progress").await;
    raw_print_job(&app, "prn_c", "shop_a", "received").await;

    // A dry run counts and writes nothing.
    let report = migrate_mongo(app.mongo(), Mode::DryRun)
        .await
        .expect("the dry run works");
    assert_eq!(report.print_jobs, 2);
    assert_eq!(status_of(&app, "prn_a").await, "in_progress");

    let report = migrate_mongo(app.mongo(), Mode::Apply)
        .await
        .expect("the migration works");
    assert_eq!(report.print_jobs, 2);
    assert_eq!(status_of(&app, "prn_a").await, "in_repair");
    assert_eq!(status_of(&app, "prn_b").await, "in_repair");
    assert_eq!(status_of(&app, "prn_c").await, "received");

    // A second run finds nothing to do.
    let report = migrate_mongo(app.mongo(), Mode::Apply)
        .await
        .expect("the second run works");
    assert_eq!(report.print_jobs, 0);
    app.mongo().drop().await.ok();
}
