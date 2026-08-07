// Mongo access for the `barcode_counters` collection only — one document
// per namespace (e.g. `"product"`). Visibility is `pub(crate)` so `service`
// can call in, but the `mod repository;` declaration in `barcode/mod.rs` is
// private, so none of this is reachable from outside this module tree.

use mongodb::{Collection, Database, bson::doc, options::ReturnDocument};

use crate::{core::error::AppResult, modules::barcode::model::BarcodeCounterDocument};

fn barcode_counters(db: &Database) -> Collection<BarcodeCounterDocument> {
    db.collection("barcode_counters")
}

/// Atomically increments and returns the next sequence number for
/// `namespace`, creating the counter starting at 1 if it doesn't exist yet
/// — the same `$inc` + `upsert` pattern as `inventory::repository::sku_counter::next_sequence`,
/// safe under concurrent generation requests without a uniqueness-retry loop.
pub(crate) async fn next_sequence(db: &Database, namespace: &str) -> AppResult<i64> {
    let updated = barcode_counters(db)
        .find_one_and_update(doc! { "_id": namespace }, doc! { "$inc": { "seq": 1i64 } })
        .upsert(true)
        .return_document(ReturnDocument::After)
        .await?;

    Ok(updated
        .expect("upsert guarantees find_one_and_update returns a document")
        .seq)
}
