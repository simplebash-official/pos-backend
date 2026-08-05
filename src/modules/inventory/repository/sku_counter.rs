// Mongo access for the `sku_counters` collection — an internal
// auto-increment sequence table, not a business entity, so unlike every
// other document in this module it does *not* get a generated `key: String`
// (see CLAUDE.md's key-prefix convention). Its `_id` is itself the
// meaningful key: the derived SKU prefix code (e.g. "PHO-SCR").

use mongodb::{Collection, Database, bson::doc, options::ReturnDocument};
use serde::{Deserialize, Serialize};

use crate::core::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkuCounterDocument {
    #[serde(rename = "_id")]
    code: String,
    seq: i64,
}

fn sku_counters(db: &Database) -> Collection<SkuCounterDocument> {
    db.collection("sku_counters")
}

/// Atomically increments and returns the next sequence number for `code`,
/// creating the counter starting at 1 if it doesn't exist yet. `$inc` on a
/// single document is atomic in MongoDB, so concurrent product creations
/// under the same derived prefix never receive the same sequence number —
/// this is what `service::sku::generate_sku` relies on for SKU uniqueness.
pub(crate) async fn next_sequence(db: &Database, code: &str) -> AppResult<i64> {
    let updated = sku_counters(db)
        .find_one_and_update(doc! { "_id": code }, doc! { "$inc": { "seq": 1i64 } })
        .upsert(true)
        .return_document(ReturnDocument::After)
        .await?;

    Ok(updated
        .expect("upsert guarantees find_one_and_update returns a document")
        .seq)
}
