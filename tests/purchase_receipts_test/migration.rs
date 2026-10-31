//! Migration tests.

use simplebash_pos_backend::core::logging::SqlLogging;

use crate::common;

/// A desktop database from before receipts (invoice number and notes on the purchase row, no
/// header table) gets receipts on startup: purchases of one supplier, invoice number and day
/// share one, a purchase without a number has its own, the keys are derived from the purchases
/// so every device derives the same ones, and a second start changes nothing.
#[tokio::test]
async fn a_sqlite_database_from_before_receipts_is_migrated() {
    use simplebash_pos_backend::clients::sqlite;
    use sqlx::Executor;

    let path = std::env::temp_dir().join(format!("pos_receipt_mig_{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("the awaited step succeeds");

    // Recreate the old shape on top of a fresh schema.
    for statement in [
        "DROP TABLE purchases",
        "DROP TABLE purchase_receipts",
        "CREATE TABLE purchases (
            key TEXT PRIMARY KEY, id TEXT NOT NULL, supplier_key TEXT NOT NULL, product_key TEXT NOT NULL,
            quantity INTEGER NOT NULL, unit_cost_cents INTEGER NOT NULL, selling_price_cents INTEGER NOT NULL,
            date TEXT NOT NULL, reference_no TEXT, notes TEXT, version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT, updated_by_device TEXT)",
        "INSERT INTO purchases (key, id, supplier_key, product_key, quantity, unit_cost_cents, selling_price_cents, date, reference_no, notes, created_at, updated_at) VALUES
            ('pur_bbbbbbbbbbbb', '65f1a1a1a1a1a1a1a1a1a1d1', 'sup_1', 'prod_1', 2, 100, 150, '2026-01-01T00:00:00+00:00', 'INV-1', 'first', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00'),
            ('pur_aaaaaaaaaaaa', '65f1a1a1a1a1a1a1a1a1a1d2', 'sup_1', 'prod_2', 3, 200, 250, '2026-01-01T00:00:00+00:00', ' INV-1 ', NULL, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00'),
            ('pur_cccccccccccc', '65f1a1a1a1a1a1a1a1a1a1d3', 'sup_1', 'prod_1', 1, 100, 150, '2026-01-01T00:00:00+00:00', NULL, NULL, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00'),
            ('pur_dddddddddddd', '65f1a1a1a1a1a1a1a1a1a1d4', 'sup_1', 'prod_1', 1, 100, 150, '2026-01-02T00:00:00+00:00', 'INV-1', NULL, '2026-01-02T00:00:00+00:00', '2026-01-02T00:00:00+00:00')",
    ] {
        pool.execute(statement).await.expect("the awaited step succeeds");
    }

    sqlite::init_db(&pool)
        .await
        .expect("the awaited step succeeds");

    let lines: Vec<(String, String)> =
        sqlx::query_as("SELECT key, receipt_key FROM purchases ORDER BY key")
            .fetch_all(&pool)
            .await
            .expect("the awaited step succeeds");
    assert_eq!(
        lines,
        vec![
            ("pur_aaaaaaaaaaaa".into(), "prc_leg_pur_aaaaaaaaaaaa".into()),
            ("pur_bbbbbbbbbbbb".into(), "prc_leg_pur_aaaaaaaaaaaa".into()),
            ("pur_cccccccccccc".into(), "prc_leg_pur_cccccccccccc".into()),
            ("pur_dddddddddddd".into(), "prc_leg_pur_dddddddddddd".into()),
        ]
    );
    let receipts: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT key, receipt_number, reference_no, notes FROM purchase_receipts ORDER BY key",
    )
    .fetch_all(&pool)
    .await
    .expect("the awaited step succeeds");
    assert_eq!(
        receipts,
        vec![
            (
                "prc_leg_pur_aaaaaaaaaaaa".into(),
                "PUR-Laaaaaaaaaa".into(),
                Some("INV-1".into()),
                Some("first".into())
            ),
            (
                "prc_leg_pur_cccccccccccc".into(),
                "PUR-Lcccccccccc".into(),
                None,
                None
            ),
            (
                "prc_leg_pur_dddddddddddd".into(),
                "PUR-Ldddddddddd".into(),
                Some("INV-1".into()),
                None
            ),
        ]
    );
    // The capture triggers come back after the backfill dropped them.
    let triggers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = 'sync_purchase_receipts_ins'",
    )
    .fetch_one(&pool)
    .await
    .expect("the awaited step succeeds");
    assert_eq!(triggers, 1);

    // Idempotent: a second start finds nothing to migrate.
    sqlite::init_db(&pool)
        .await
        .expect("the awaited step succeeds");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM purchase_receipts")
        .fetch_one(&pool)
        .await
        .expect("the awaited step succeeds");
    assert_eq!(count, 3);
    pool.close().await;
    std::fs::remove_file(&path).ok();
}

/// The same history in Atlas gets the same receipts a desktop derives, and a second run
/// changes nothing.
#[tokio::test]
async fn a_mongo_database_from_before_receipts_is_migrated() {
    use mongodb::bson::{DateTime, Document, doc};
    use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
    use simplebash_pos_backend::modules::purchases::service::receipt_migration::migrate_mongo;

    let app = common::spawn_app_isolated().await;
    let purchases = app.mongo().collection::<Document>("purchases");
    let day = DateTime::parse_rfc3339_str("2026-01-01T00:00:00Z").expect("the step succeeds");
    let next_day = DateTime::parse_rfc3339_str("2026-01-02T00:00:00Z").expect("the step succeeds");
    let legacy = |key: &str, reference: Option<&str>, notes: Option<&str>, date: DateTime| {
        let mut document = doc! {
            "key": key, "supplier_key": "sup_1", "product_key": "prod_1", "quantity": 2_i64,
            "unit_cost_cents": 100_i64, "selling_price_cents": 150_i64, "date": date,
            "version": 1_i64, "created_at": date, "updated_at": date,
        };
        if let Some(reference) = reference {
            document.insert("reference_no", reference);
        }
        if let Some(notes) = notes {
            document.insert("notes", notes);
        }
        document
    };
    purchases
        .insert_many(vec![
            legacy("pur_bbbbbbbbbbbb", Some("INV-1"), Some("first"), day),
            legacy("pur_aaaaaaaaaaaa", Some(" INV-1 "), None, day),
            legacy("pur_cccccccccccc", None, None, day),
            legacy("pur_dddddddddddd", Some("INV-1"), None, next_day),
        ])
        .await
        .expect("the awaited step succeeds");

    let dry = migrate_mongo(app.mongo(), Mode::DryRun)
        .await
        .expect("the awaited step succeeds");
    assert_eq!((dry.purchases, dry.receipts), (4, 3));
    assert_eq!(
        app.mongo()
            .collection::<Document>("purchase_receipts")
            .count_documents(doc! {})
            .await
            .expect("the awaited step succeeds"),
        0,
        "a dry run writes nothing"
    );

    let done = migrate_mongo(app.mongo(), Mode::Apply)
        .await
        .expect("the awaited step succeeds");
    assert_eq!((done.purchases, done.receipts), (4, 3));
    let mut keys: Vec<(String, String)> = Vec::new();
    let mut cursor = purchases
        .find(doc! {})
        .sort(doc! { "key": 1 })
        .await
        .expect("the awaited step succeeds");
    while cursor.advance().await.expect("the awaited step succeeds") {
        let document = cursor.deserialize_current().expect("a match exists");
        assert!(
            document.get("reference_no").is_none(),
            "the invoice number moved to the receipt"
        );
        keys.push((
            document
                .get_str("key")
                .expect("the document has this string field")
                .to_string(),
            document
                .get_str("receipt_key")
                .expect("the document has this string field")
                .to_string(),
        ));
    }
    assert_eq!(
        keys,
        vec![
            (
                "pur_aaaaaaaaaaaa".to_string(),
                "prc_leg_pur_aaaaaaaaaaaa".to_string()
            ),
            (
                "pur_bbbbbbbbbbbb".to_string(),
                "prc_leg_pur_aaaaaaaaaaaa".to_string()
            ),
            (
                "pur_cccccccccccc".to_string(),
                "prc_leg_pur_cccccccccccc".to_string()
            ),
            (
                "pur_dddddddddddd".to_string(),
                "prc_leg_pur_dddddddddddd".to_string()
            ),
        ]
    );
    let receipt = app
        .mongo()
        .collection::<Document>("purchase_receipts")
        .find_one(doc! { "key": "prc_leg_pur_aaaaaaaaaaaa" })
        .await
        .expect("the awaited step succeeds")
        .expect("the step succeeds");
    assert_eq!(
        receipt
            .get_str("receipt_number")
            .expect("the document has this string field"),
        "PUR-Laaaaaaaaaa"
    );
    assert_eq!(
        receipt
            .get_str("reference_no")
            .expect("the document has this string field"),
        "INV-1"
    );
    assert_eq!(
        receipt
            .get_str("notes")
            .expect("the document has this string field"),
        "first"
    );

    let again = migrate_mongo(app.mongo(), Mode::Apply)
        .await
        .expect("the awaited step succeeds");
    assert_eq!((again.purchases, again.receipts), (0, 0));
    assert_eq!(
        app.mongo()
            .collection::<Document>("purchase_receipts")
            .count_documents(doc! {})
            .await
            .expect("the awaited step succeeds"),
        3
    );
    app.mongo().drop().await.ok();
}
