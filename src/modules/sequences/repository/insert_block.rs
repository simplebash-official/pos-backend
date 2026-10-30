use crate::clients::db::Db;
use crate::clients::tenant_db::ScopedCollection;
use crate::core::error::AppResult;
use crate::modules::sequences::model::SequenceBlockDocument;

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
