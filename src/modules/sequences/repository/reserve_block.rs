use crate::clients::db::Db;
use crate::clients::tenant_db::ScopedCollection;
use crate::core::error::AppResult;
use crate::modules::sequences::model::SequenceCounterDocument;
use mongodb::bson::doc;
use mongodb::options::ReturnDocument;

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
                    doc! { "_id": super::super::counter_id(db, name) },
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
