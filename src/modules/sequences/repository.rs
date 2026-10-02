use crate::clients::tenant_db::ScopedCollection;
use crate::{
    clients::db::Db,
    core::error::AppResult,
    modules::sequences::model::{SequenceBlockDocument, SequenceCounterDocument},
};
use mongodb::{bson::doc, options::ReturnDocument};

pub(crate) async fn reserve_block(
    db: &Db,
    name: &str,
    prefix: &str,
    padding: usize,
    block_size: i64,
) -> AppResult<(i64, i64)> {
    match db {
        Db::Mongo(db) => {
            let counters: ScopedCollection<SequenceCounterDocument> =
                db.collection("sequence_counters");
            let now = mongodb::bson::DateTime::now();

            let updated = counters
                .find_one_and_update(
                    doc! { "_id": super::counter_id(db, name) },
                    doc! {
                        "$inc": { "next_val": block_size },
                        "$setOnInsert": { "prefix": prefix, "padding": padding as i32 },
                        "$set": { "updated_at": now }
                    },
                )
                .upsert(true)
                .return_document(ReturnDocument::After)
                .await?;

            let next_val = updated.map(|d| d.next_val).unwrap_or(block_size);
            let start = next_val - block_size + 1;
            let end = next_val;
            Ok((start, end))
        }
        Db::Sqlite(pool) => {
            let now = chrono::Utc::now().to_rfc3339();
            let row: (i64,) = sqlx::query_as(
                r#"
                INSERT INTO sequence_counters (name, prefix, padding, next_val, updated_at)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT(name) DO UPDATE SET
                    next_val = sequence_counters.next_val + $4,
                    updated_at = $5
                RETURNING next_val
                "#,
            )
            .bind(name)
            .bind(prefix)
            .bind(padding as i64)
            .bind(block_size)
            .bind(&now)
            .fetch_one(pool)
            .await?;

            let next_val = row.0;
            let start = next_val - block_size + 1;
            let end = next_val;
            Ok((start, end))
        }
    }
}

pub(crate) async fn insert_block(
    db: &Db,
    block: SequenceBlockDocument,
) -> AppResult<SequenceBlockDocument> {
    match db {
        Db::Mongo(db) => {
            let blocks: ScopedCollection<SequenceBlockDocument> = db.collection("sequence_blocks");
            blocks.insert_one(&block).await?;
            Ok(block)
        }
        Db::Sqlite(pool) => {
            sqlx::query(
                r#"
                INSERT INTO sequence_blocks (key, counter_name, device_id, start_seq, end_seq, current_seq, reserved_at, expires_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(&block.key)
            .bind(&block.name)
            .bind(&block.device_id)
            .bind(block.start)
            .bind(block.end)
            .bind(block.start)
            .bind(block.created_at.to_chrono().to_rfc3339())
            .bind(block.expires_at.to_chrono().to_rfc3339())
            .execute(pool)
            .await?;
            Ok(block)
        }
    }
}
