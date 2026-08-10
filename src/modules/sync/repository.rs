use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc},
};

use crate::{core::error::AppResult, modules::sync::cursor::DecodedCursor};

pub(crate) async fn fetch_collection_changes(
    db: &Database,
    collection_name: &str,
    cursor: Option<&DecodedCursor>,
    limit: i64,
) -> AppResult<Vec<Document>> {
    let collection: Collection<Document> = db.collection(collection_name);

    let filter = match cursor {
        None => doc! { "deleted_at": { "$exists": false } },
        Some(c) => {
            let bson_time = BsonDateTime::from_chrono(c.timestamp);
            if let Some(ref k) = c.key {
                doc! {
                    "$or": [
                        { "updated_at": { "$gt": bson_time } },
                        { "updated_at": bson_time, "key": { "$gt": k } }
                    ]
                }
            } else {
                doc! { "updated_at": { "$gt": bson_time } }
            }
        }
    };

    let mut db_cursor = collection
        .find(filter)
        .sort(doc! { "updated_at": 1, "key": 1 })
        .limit(limit)
        .await?;

    let mut items = Vec::new();
    while let Some(doc) = db_cursor.try_next().await? {
        items.push(doc);
    }

    Ok(items)
}
