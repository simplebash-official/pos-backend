use crate::clients::db::Db;
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::error::AppResult;
use mongodb::bson::doc;
use mongodb::options::ReturnDocument;
use serde::{Deserialize, Serialize};

/// Mongo document shape tracking auto-increment SKU sequence per prefix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SkuCounterDocument {
    /// Derived category and subcategory prefix code (e.g. "PHO-SCR").
    #[serde(rename = "_id")]
    pub(super) code: String,
    /// Last generated sequence number for this prefix.
    pub(super) seq: i64,
}

fn sku_counters(db: &TenantDatabase) -> ScopedCollection<SkuCounterDocument> {
    db.collection("sku_counters")
}

/// `$inc`s the counter for `code` by `by` and returns the new value, creating it when missing.
///
/// Two requests creating the *same new* counter at once both try to insert it, and the loser
/// fails with a duplicate-key error (E11000) instead of incrementing. The counter exists by then,
/// so repeating the call increments it normally; this is the retry MongoDB documents for
/// concurrent upserts. It matters now that sample products are created concurrently.
pub(super) async fn bump(db: &TenantDatabase, code: &str, by: i64) -> AppResult<i64> {
    let id = crate::modules::sequences::counter_id(db, code);
    let mut attempt = 0;
    loop {
        let result = sku_counters(db)
            .find_one_and_update(doc! { "_id": &id }, doc! { "$inc": { "seq": by } })
            .upsert(true)
            .return_document(ReturnDocument::After)
            .await;
        match result {
            Ok(updated) => {
                return Ok(updated
                    .expect("upsert guarantees find_one_and_update returns a document")
                    .seq);
            }
            Err(e) if attempt < 3 && is_duplicate_key(&e) => attempt += 1,
            Err(e) => return Err(e.into()),
        }
    }
}

fn is_duplicate_key(err: &mongodb::error::Error) -> bool {
    use mongodb::error::{ErrorKind, WriteFailure};
    match err.kind.as_ref() {
        ErrorKind::Command(c) => c.code == 11000,
        ErrorKind::Write(WriteFailure::WriteError(w)) => w.code == 11000,
        _ => false,
    }
}

/// Atomically increments and returns the next sequence number for `code`,
/// creating the counter starting at 1 if it doesn't exist yet.
pub(crate) async fn next_sequence(db: &Db, code: &str) -> AppResult<i64> {
    match db {
        Db::Mongo(db) => bump(db, code, 1).await,
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

/// Atomically reserves a contiguous block of `block_size` sequence numbers
/// for `code`, drawn from the exact same counter row `next_sequence`
/// increments one at a time — a block a linked device pre-fetches and a SKU
/// generated directly by `generate_sku` (unlinked, or online with no block
/// held) can therefore never land on the same number.
pub(crate) async fn reserve_block(db: &Db, code: &str, block_size: i64) -> AppResult<(i64, i64)> {
    match db {
        Db::Mongo(db) => {
            let seq = bump(db, code, block_size).await?;
            Ok((seq - block_size + 1, seq))
        }
        Db::Sqlite(pool) => {
            let seq: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO sku_counters (prefix, seq) VALUES (?, ?)
                ON CONFLICT(prefix) DO UPDATE SET seq = seq + ?
                RETURNING seq
                "#,
            )
            .bind(code)
            .bind(block_size)
            .bind(block_size)
            .fetch_one(pool)
            .await?;
            Ok((seq - block_size + 1, seq))
        }
    }
}
