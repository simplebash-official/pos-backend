//! Completeness of stored documents: `scan` finds documents that lack fields and, in apply
//! mode, writes them. Runs against a real MongoDB (`MONGODB_URI`) in a throwaway database
//! that is dropped afterwards.

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use simplebash_pos_backend::clients::mongo::connect;
use simplebash_pos_backend::modules::customers::model::CustomerDocument;
use simplebash_pos_backend::modules::stored_shape::{Mode, scan};
use uuid::Uuid;

async fn throwaway() -> mongodb::Database {
    dotenvy::dotenv().ok();
    let uri = std::env::var("MONGODB_URI")
        .expect("Mongo-backed tests need MONGODB_URI (use `make check-atlas`)");
    let name = format!("jtshape_{}", &Uuid::new_v4().simple().to_string()[..20]);
    connect(&uri, &name).await.expect("connect to MongoDB")
}

fn customer(key: &str) -> CustomerDocument {
    let now = BsonDateTime::now();
    CustomerDocument {
        id: None,
        key: key.to_string(),
        name: "Shape Test".to_string(),
        contact_person: None,
        primary_phone: "0770000000".to_string(),
        secondary_phone: None,
        email: None,
        address: None,
        tags: vec![],
        notes: None,
        outstanding_balance_cents: 0,
        total_purchases_cents: 0,
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
        updated_by_device: None,
    }
}

fn collection_of(db: &mongodb::Database) -> mongodb::Collection<Document> {
    db.collection::<Document>("customers")
}

#[tokio::test]
async fn a_complete_database_reports_clean() {
    let db = throwaway().await;
    db.collection::<CustomerDocument>("customers")
        .insert_one(customer("cus_complete"))
        .await
        .expect("insert a complete customer");

    let report = scan(&db, Mode::Report).await.expect("scan");

    assert!(report.is_clean(), "{}", report.render("lack"));
    db.drop().await.expect("drop the throwaway database");
}

#[tokio::test]
async fn missing_fields_are_reported_then_written_by_apply() {
    let db = throwaway().await;
    db.collection::<CustomerDocument>("customers")
        .insert_one(customer("cus_old"))
        .await
        .expect("insert a customer");
    collection_of(&db)
        .update_one(
            doc! { "key": "cus_old" },
            doc! { "$unset": { "tags": "", "version": "", "total_purchases_cents": "" } },
        )
        .await
        .expect("remove fields an old document would not have");

    let report_only = scan(&db, Mode::Report).await.expect("report scan");
    let customers = report_only
        .collections
        .iter()
        .find(|c| c.name == "customers")
        .expect("customers are scanned");
    assert_eq!(customers.incomplete, 1);
    assert_eq!(customers.missing.get("tags"), Some(&1));
    assert_eq!(customers.missing.get("version"), Some(&1));
    assert_eq!(customers.missing.get("total_purchases_cents"), Some(&1));
    let untouched = collection_of(&db)
        .find_one(doc! { "key": "cus_old" })
        .await
        .expect("read")
        .expect("the customer exists");
    assert!(
        !untouched.contains_key("version"),
        "Report mode must not write"
    );

    scan(&db, Mode::Apply).await.expect("apply scan");

    let repaired = collection_of(&db)
        .find_one(doc! { "key": "cus_old" })
        .await
        .expect("read")
        .expect("the customer exists");
    assert_eq!(repaired.get_i64("version").expect("version written"), 1);
    assert_eq!(
        repaired
            .get_i64("total_purchases_cents")
            .expect("total written"),
        0
    );
    assert!(repaired.get_array("tags").expect("tags written").is_empty());
    let after = scan(&db, Mode::Report).await.expect("rescan");
    assert!(after.is_clean(), "{}", after.render("lack"));
    let rerun = scan(&db, Mode::Apply).await.expect("second apply");
    assert!(rerun.is_clean(), "a second apply finds nothing to do");
    db.drop().await.expect("drop the throwaway database");
}

#[tokio::test]
async fn a_document_with_no_safe_backfill_is_left_alone_and_counted() {
    let db = throwaway().await;
    collection_of(&db)
        .insert_one(doc! { "key": "cus_broken", "name": "No phone" })
        .await
        .expect("insert a broken customer");

    let report = scan(&db, Mode::Apply).await.expect("scan");

    let customers = report
        .collections
        .iter()
        .find(|c| c.name == "customers")
        .expect("customers are scanned");
    assert_eq!(customers.unrepairable, 1);
    assert!(!report.is_clean());
    let stored = collection_of(&db)
        .find_one(doc! { "key": "cus_broken" })
        .await
        .expect("read")
        .expect("the customer exists");
    assert!(
        !stored.contains_key("version"),
        "an unrepairable document is not partly rewritten"
    );
    db.drop().await.expect("drop the throwaway database");
}
