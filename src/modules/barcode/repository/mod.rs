// Mongo access for the `barcode_counters` collection only — one document
// per namespace (e.g. `"product"`). Visibility is `pub(crate)` so `service`
// can call in, but the `mod repository;` declaration in `barcode/mod.rs` is
// private, so none of this is reachable from outside this module tree.

use mongodb::{Collection, bson::doc, options::ReturnDocument};

use crate::{
    clients::db::Db, core::error::AppResult, modules::barcode::model::BarcodeCounterDocument,
};

fn barcode_counters(db: &mongodb::Database) -> Collection<BarcodeCounterDocument> {
    db.collection("barcode_counters")
}

/// Atomically increments and returns the next sequence number for
/// `namespace`, creating the counter starting at 1 if it doesn't exist yet
/// — the same `$inc` + `upsert` pattern as `inventory::repository::sku_counter::next_sequence`,
/// safe under concurrent generation requests without a uniqueness-retry loop.
pub(crate) async fn next_sequence(db: &Db, namespace: &str) -> AppResult<i64> {
    match db {
        Db::Mongo(db) => {
            let updated = barcode_counters(db)
                .find_one_and_update(doc! { "_id": namespace }, doc! { "$inc": { "seq": 1i64 } })
                .upsert(true)
                .return_document(ReturnDocument::After)
                .await?;

            Ok(updated
                .expect("upsert guarantees find_one_and_update returns a document")
                .seq)
        }
        Db::Sqlite(pool) => {
            let row: (i64,) = sqlx::query_as(
                r#"
                INSERT INTO barcode_counters (prefix, seq)
                VALUES ($1, 1)
                ON CONFLICT(prefix) DO UPDATE SET
                    seq = barcode_counters.seq + 1
                RETURNING seq
                "#,
            )
            .bind(namespace)
            .fetch_one(pool)
            .await?;
            Ok(row.0)
        }
    }
}
