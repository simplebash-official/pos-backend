use crate::{
    core::error::AppResult,
    modules::sequences::model::{SequenceBlockDocument, SequenceCounterDocument},
};
use mongodb::{Collection, Database, bson::doc, options::ReturnDocument};

pub(crate) async fn reserve_block(
    db: &Database,
    name: &str,
    prefix: &str,
    padding: usize,
    block_size: i64,
) -> AppResult<(i64, i64)> {
    let counters: Collection<SequenceCounterDocument> = db.collection("sequence_counters");
    let now = mongodb::bson::DateTime::now();

    let updated = counters
        .find_one_and_update(
            doc! { "_id": name },
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

pub(crate) async fn insert_block(
    db: &Database,
    block: SequenceBlockDocument,
) -> AppResult<SequenceBlockDocument> {
    let blocks: Collection<SequenceBlockDocument> = db.collection("sequence_blocks");
    blocks.insert_one(&block).await?;
    Ok(block)
}
