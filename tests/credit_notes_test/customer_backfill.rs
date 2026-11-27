//! The one-time MongoDB copy of a customer onto credit notes saved without one, across shops.

use crate::common;
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use simplebash_pos_backend::clients;
use simplebash_pos_backend::modules::billing::migration::migrate_credit_note_customers_mongo;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;

async fn insert(db: &Database, collection: &str, documents: Vec<Document>) {
    db.collection::<Document>(collection)
        .insert_many(documents)
        .await
        .expect("the rows are stored");
}

/// Every credit note as `(key, customer key, customer name)`, by key.
async fn customers_of_notes(db: &Database) -> Vec<(String, Option<String>, Option<String>)> {
    let notes: Vec<Document> = db
        .collection::<Document>("credit_notes")
        .find(doc! {})
        .sort(doc! { "key": 1 })
        .await
        .expect("the notes are read")
        .try_collect()
        .await
        .expect("the notes are read");
    notes
        .iter()
        .map(|note| {
            let text = |field: &str| note.get_str(field).ok().map(str::to_string);
            (
                text("key").expect("a note has a key"),
                text("customer_key"),
                text("customer_name_snapshot"),
            )
        })
        .collect()
}

fn row(
    key: &str,
    customer: Option<&str>,
    name: Option<&str>,
) -> (String, Option<String>, Option<String>) {
    (
        key.to_string(),
        customer.map(str::to_string),
        name.map(str::to_string),
    )
}

#[tokio::test]
async fn mongo_gives_old_credit_notes_the_customer_of_their_own_shop() {
    dotenvy::dotenv().ok();
    let name = format!("jtcnb_{}", &uuid::Uuid::new_v4().simple().to_string()[..20]);
    let db = clients::mongo::connect(&common::mongo_uri(), &name)
        .await
        .expect("the throwaway database opens");
    // Two shops number their rows alike; one database is single-shop (no `tenant_id`).
    insert(
        &db,
        "invoices",
        vec![
            doc! { "tenant_id": "a", "key": "inv_1", "customer_key": "cust_a",
            "customer_name_snapshot": "Anne" },
            doc! { "tenant_id": "b", "key": "inv_1", "customer_key": "cust_b",
            "customer_name_snapshot": "Bob" },
            doc! { "key": "inv_typed", "customer_name_snapshot": "Typed" },
            doc! { "key": "inv_walk_in" },
        ],
    )
    .await;
    insert(
        &db,
        "credit_notes",
        vec![
            doc! { "tenant_id": "a", "key": "cn_1", "invoice_key": "inv_1" },
            doc! { "tenant_id": "b", "key": "cn_2", "invoice_key": "inv_1" },
            doc! { "key": "cn_3", "invoice_key": "inv_typed" },
            doc! { "key": "cn_4", "invoice_key": "inv_walk_in" },
            doc! { "key": "cn_5", "no_receipt": true },
        ],
    )
    .await;
    let before = customers_of_notes(&db).await;

    let dry = migrate_credit_note_customers_mongo(&db, Mode::DryRun)
        .await
        .expect("the dry run reads");
    assert_eq!(dry.credit_notes, 3);
    assert_eq!(customers_of_notes(&db).await, before);

    let applied = migrate_credit_note_customers_mongo(&db, Mode::Apply)
        .await
        .expect("the copy runs");
    assert_eq!(applied.credit_notes, 3);
    assert_eq!(
        customers_of_notes(&db).await,
        vec![
            row("cn_1", Some("cust_a"), Some("Anne")),
            row("cn_2", Some("cust_b"), Some("Bob")),
            row("cn_3", None, Some("Typed")),
            row("cn_4", None, None),
            row("cn_5", None, None),
        ]
    );
    let again = migrate_credit_note_customers_mongo(&db, Mode::Apply)
        .await
        .expect("a second run reads");
    assert_eq!(again.credit_notes, 0);
    db.drop().await.expect("the throwaway database is dropped");
}
