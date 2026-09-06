// Mongo access for the `sku_counters` collection — an internal
// auto-increment sequence table, not a business entity, so unlike every
// other document in this module it does *not* get a generated `key: String`
// (see CLAUDE.md's key-prefix convention). Its `_id` is itself the
// meaningful key: the derived SKU prefix code (e.g. "PHO-SCR").

use mongodb::{Collection, Database, bson::doc, options::ReturnDocument};
use serde::{Deserialize, Serialize};

use crate::{clients::db::Db, core::error::AppResult};

/// Mongo document shape tracking auto-increment SKU sequence per prefix.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkuCounterDocument {
    /// Derived category and subcategory prefix code (e.g. "PHO-SCR").
    #[serde(rename = "_id")]
    code: String,
    /// Last generated sequence number for this prefix.
    seq: i64,
}

fn sku_counters(db: &Database) -> Collection<SkuCounterDocument> {
    db.collection("sku_counters")
}

/// Atomically increments and returns the next sequence number for `code`,
/// creating the counter starting at 1 if it doesn't exist yet.
pub(crate) async fn next_sequence(db: &Db, code: &str) -> AppResult<i64> {
    match db {
        Db::Mongo(db) => {
            let updated = sku_counters(db)
                .find_one_and_update(doc! { "_id": code }, doc! { "$inc": { "seq": 1i64 } })
                .upsert(true)
                .return_document(ReturnDocument::After)
                .await?;

            Ok(updated
                .expect("upsert guarantees find_one_and_update returns a document")
                .seq)
        }
        Db::Sqlite(pool) => {
            let seq: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO sku_counters (prefix, seq) VALUES (?, 1)
                ON CONFLICT(prefix) DO UPDATE SET seq = seq + 1
                RETURNING seq
                "#,
            )
            .bind(code)
            .fetch_one(pool)
            .await?;

            Ok(seq)
        }
    }
}
