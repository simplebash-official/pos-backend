//! Repair jobs and print jobs saved before a customer was required get one: a live SQLite
//! database at startup and a MongoDB database holding several shops.

mod common;

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use simplebash_pos_backend::clients::sqlite;
use simplebash_pos_backend::core::logging::SqlLogging;
use simplebash_pos_backend::core::tenancy::TENANT_FIELD;
use simplebash_pos_backend::modules::customers::migration::{
    migrate_mongo, migrated_customer_key, phone_identity,
};
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use sqlx::Executor;

/// The `repairs` and `print_jobs` tables as they were when a ticket could be saved without a
/// customer link.
const OLD_TABLES: [&str; 4] = [
    "DROP TABLE repairs",
    "CREATE TABLE repairs (key TEXT PRIMARY KEY, id TEXT NOT NULL, ticket_number TEXT NOT NULL \
     UNIQUE, customer_key TEXT, customer_name TEXT NOT NULL, customer_phone TEXT NOT NULL, \
     device_model TEXT NOT NULL, serial_number TEXT, issue_description TEXT NOT NULL, \
     promised_ready_at TEXT, status TEXT NOT NULL, kind JSON NOT NULL, items JSON NOT NULL, \
     price_cents INTEGER NOT NULL, cost_cents INTEGER NOT NULL, assigned_employee_id TEXT, \
     assigned_employee_name TEXT, split_type TEXT, split_value REAL, version INTEGER NOT NULL \
     DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT, \
     updated_by_device TEXT)",
    "DROP TABLE print_jobs",
    "CREATE TABLE print_jobs (key TEXT PRIMARY KEY, id TEXT NOT NULL, ticket_number TEXT NOT \
     NULL UNIQUE, customer_key TEXT, customer_name TEXT NOT NULL, customer_phone TEXT NOT NULL, \
     job_type TEXT NOT NULL, quantity INTEGER NOT NULL DEFAULT 1, promised_ready_at TEXT, \
     status TEXT NOT NULL, estimated_cost_cents INTEGER NOT NULL, material_cost_cents INTEGER, \
     assigned_employee_id TEXT, assigned_employee_name TEXT, split_type TEXT, split_value REAL, \
     version INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, \
     deleted_at TEXT, updated_by_device TEXT)",
];

fn repair_row(
    key: &str,
    ticket: &str,
    customer_key: &str,
    name: &str,
    phone: &str,
    day: &str,
) -> String {
    format!(
        "INSERT INTO repairs (key, id, ticket_number, customer_key, customer_name, \
         customer_phone, device_model, issue_description, status, kind, items, price_cents, \
         cost_cents, created_at, updated_at) VALUES ('{key}', '65f1a1a1a1a1a1a1a1{ticket}', \
         'REP-{ticket}', {customer_key}, '{name}', '{phone}', 'Pixel 7', 'No power', \
         'received', '{{\"kind\":\"standard\"}}', '[]', 0, 0, '{day}', '{day}')"
    )
}

fn print_row(key: &str, ticket: &str, name: &str, phone: &str, day: &str) -> String {
    format!(
        "INSERT INTO print_jobs (key, id, ticket_number, customer_name, customer_phone, \
         job_type, status, estimated_cost_cents, created_at, updated_at) VALUES ('{key}', \
         '65f1a1a1a1a1a1a1a2{ticket}', 'PRN-{ticket}', '{name}', '{phone}', 'mug', \
         'received', 1000, '{day}', '{day}')"
    )
}

fn customer_row(key: &str, name: &str, phone: &str, day: &str, deleted: Option<&str>) -> String {
    let deleted = deleted.map_or("NULL".to_string(), |d| format!("'{d}'"));
    format!(
        "INSERT INTO customers (key, id, name, primary_phone, created_at, updated_at, deleted_at) \
         VALUES ('{key}', '65f1a1a1a1a1a1a1a1a1a3{}', '{name}', '{phone}', '{day}', '{day}', \
         {deleted})",
        &key[key.len() - 2..]
    )
}

async fn linked(pool: &sqlx::SqlitePool, table: &str, key: &str) -> Option<String> {
    sqlx::query_scalar(&format!("SELECT customer_key FROM {table} WHERE key = ?"))
        .bind(key)
        .fetch_one(pool)
        .await
        .expect("the ticket is stored")
}

#[tokio::test]
async fn an_old_sqlite_database_links_every_ticket_to_a_customer() {
    let path = std::env::temp_dir().join(format!("pos_tkt_cus_{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("the database opens");

    let mut statements: Vec<String> = OLD_TABLES.iter().map(|s| (*s).to_string()).collect();
    statements.extend([
        // Two live customers with one number: the older one is chosen. A deleted customer
        // never is.
        customer_row(
            "cus_aa",
            "Nimal Old",
            "+94771234567",
            "2026-01-01T05:00:00+00:00",
            None,
        ),
        customer_row(
            "cus_bb",
            "Nimal New",
            "0771234567",
            "2026-02-01T05:00:00+00:00",
            None,
        ),
        customer_row(
            "cus_cc",
            "Kamal Gone",
            "0712223334",
            "2025-12-01T05:00:00+00:00",
            Some("2026-01-01T05:00:00+00:00"),
        ),
        // Matches an existing customer, spelled differently.
        repair_row(
            "rep_known",
            "000001",
            "NULL",
            "Nimal P",
            "077 123 4567",
            "2026-03-10T05:00:00+00:00",
        ),
        // Two tickets of a number nobody is registered under (the older ticket names them).
        repair_row(
            "rep_new",
            "000002",
            "NULL",
            "Kamal Perera",
            "0712223334",
            "2026-03-11T05:00:00+00:00",
        ),
        print_row(
            "prn_new",
            "000001",
            "Kamal P.",
            "071 222 3334",
            "2026-03-12T05:00:00+00:00",
        ),
        // Already linked: left alone.
        repair_row(
            "rep_done",
            "000003",
            "'cus_cc'",
            "Whoever",
            "0700000000",
            "2026-03-13T05:00:00+00:00",
        ),
    ]);
    for statement in statements {
        pool.execute(statement.as_str())
            .await
            .expect("the old shape is set up");
    }

    sqlite::init_db(&pool).await.expect("the migration runs");

    assert_eq!(
        linked(&pool, "repairs", "rep_known").await.as_deref(),
        Some("cus_aa")
    );
    assert_eq!(
        linked(&pool, "repairs", "rep_done").await.as_deref(),
        Some("cus_cc")
    );
    let made = migrated_customer_key(&phone_identity("0712223334"));
    assert_eq!(
        linked(&pool, "repairs", "rep_new").await.as_deref(),
        Some(made.as_str())
    );
    assert_eq!(
        linked(&pool, "print_jobs", "prn_new").await.as_deref(),
        Some(made.as_str())
    );

    let (name, phone, created_at): (String, String, String) =
        sqlx::query_as("SELECT name, primary_phone, created_at FROM customers WHERE key = ?")
            .bind(&made)
            .fetch_one(&pool)
            .await
            .expect("the customer was made");
    assert_eq!(name, "Kamal Perera");
    assert_eq!(phone, "0712223334");
    assert_eq!(created_at, "2026-03-11T05:00:00+00:00");

    // The rows load as tickets, and a second start changes nothing.
    let db = simplebash_pos_backend::clients::db::Db::Sqlite(pool.clone());
    let job = simplebash_pos_backend::modules::repairs::service::get_repair(&db, "rep_new")
        .await
        .expect("the job loads");
    assert_eq!(job.customer_key, made);
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
        .fetch_one(&pool)
        .await
        .expect("count");
    sqlite::init_db(&pool).await.expect("the second start runs");
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(before, after);
    drop(pool);
    common::remove_sqlite_files(&path);
}

async fn raw_ticket(
    app: &common::TestApp,
    collection: &str,
    key: &str,
    shop: Option<&str>,
    phone: &str,
) {
    let mut ticket = doc! {
        "key": key, "customer_name": "Kamal Perera", "customer_phone": phone,
        "created_at": BsonDateTime::now(),
    };
    if let Some(shop) = shop {
        ticket.insert(TENANT_FIELD, shop);
    }
    app.mongo()
        .collection::<Document>(collection)
        .insert_one(ticket)
        .await
        .expect("the ticket is stored");
}

async fn key_of(app: &common::TestApp, collection: &str, key: &str) -> Option<String> {
    app.mongo()
        .collection::<Document>(collection)
        .find_one(doc! { "key": key })
        .await
        .expect("the ticket is read")
        .expect("the ticket exists")
        .get_str("customer_key")
        .ok()
        .map(str::to_string)
}

#[tokio::test]
async fn a_mongo_database_links_tickets_shop_by_shop() {
    let app = common::spawn_app_isolated().await;
    raw_ticket(&app, "repairs", "rep_a", Some("shop_a"), "0712223334").await;
    raw_ticket(&app, "print_jobs", "prn_a", Some("shop_a"), "071 222 3334").await;
    raw_ticket(&app, "repairs", "rep_b", Some("shop_b"), "0712223334").await;

    // A dry run counts and writes nothing.
    let report = migrate_mongo(app.mongo(), Mode::DryRun)
        .await
        .expect("the dry run works");
    assert_eq!(report.tickets, 3);
    assert_eq!(report.customers_created, 2);
    assert_eq!(key_of(&app, "repairs", "rep_a").await, None);

    let report = migrate_mongo(app.mongo(), Mode::Apply)
        .await
        .expect("the migration works");
    assert_eq!((report.tickets, report.customers_created), (3, 2));
    let made = migrated_customer_key(&phone_identity("0712223334"));
    for (collection, key) in [
        ("repairs", "rep_a"),
        ("print_jobs", "prn_a"),
        ("repairs", "rep_b"),
    ] {
        assert_eq!(
            key_of(&app, collection, key).await.as_deref(),
            Some(made.as_str())
        );
    }
    // One customer per shop, each in its own shop.
    let customers = app.mongo().collection::<Document>("customers");
    for shop in ["shop_a", "shop_b"] {
        let count = customers
            .count_documents(doc! { "key": &made, TENANT_FIELD: shop })
            .await
            .expect("count");
        assert_eq!(count, 1, "{shop}");
    }

    // A second run finds nothing to do.
    let report = migrate_mongo(app.mongo(), Mode::Apply)
        .await
        .expect("the second run works");
    assert_eq!((report.tickets, report.customers_created), (0, 0));
    app.mongo().drop().await.ok();
}
