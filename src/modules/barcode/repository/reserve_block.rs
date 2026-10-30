use super::barcode_counters::barcode_counters;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use mongodb::bson::doc;
use mongodb::options::ReturnDocument;

/// Atomically reserves a contiguous block of `block_size` sequence numbers
/// for `namespace`, drawn from the exact same counter row `next_sequence`
/// increments one at a time — a block a linked device pre-fetches and a
/// barcode generated directly (unlinked, or online with no block held) can
/// therefore never land on the same number.
pub(crate) async fn reserve_block(
    db: &Db,
    namespace: &str,
    block_size: i64,
) -> AppResult<(i64, i64)> {
    match db {
        Db::Mongo(db) => {
            let updated = barcode_counters(db)
                .find_one_and_update(
                    doc! { "_id": crate::modules::sequences::counter_id(db, namespace) },
                    doc! { "$inc": { "seq": block_size } },
                )
                .upsert(true)
                .return_document(ReturnDocument::After)
                .await?;

            let seq = updated
                .expect("upsert guarantees find_one_and_update returns a document")
                .seq;
            Ok((seq - block_size + 1, seq))
        }
        Db::Sqlite(pool) => {
            let row: (i64,) = sqlx::query_as(
                r#"
                INSERT INTO barcode_counters (prefix, seq)
                VALUES (?, ?)
                ON CONFLICT(prefix) DO UPDATE SET
                    seq = barcode_counters.seq + ?
                RETURNING seq
                "#,
            )
            .bind(namespace)
            .bind(block_size)
            .bind(block_size)
            .fetch_one(pool)
            .await?;
            Ok((row.0 - block_size + 1, row.0))
        }
    }
}
