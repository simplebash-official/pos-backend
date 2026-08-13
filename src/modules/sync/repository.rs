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
        // A snapshot must not carry tombstones: the client has no row to
        // delete yet, so a deleted doc would just be noise. Deltas keep
        // them, since that is the only way a peer learns about a delete.
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

/// The newest `(updated_at, key)` pair in a collection, which is what a
/// cursor has to be built from. Sorted the same way `fetch_collection_changes`
/// pages, only descending, so the pair is exactly the point a delta pull
/// would resume from after consuming every existing row.
pub(crate) async fn fetch_latest_change_marker(
    db: &Database,
    collection_name: &str,
) -> AppResult<Option<(BsonDateTime, String)>> {
    let collection: Collection<Document> = db.collection(collection_name);
    let doc = collection
        .find_one(doc! { "updated_at": { "$exists": true } })
        .sort(doc! { "updated_at": -1, "key": -1 })
        .await?;

    Ok(doc.and_then(|d| {
        let updated_at = d.get_datetime("updated_at").copied().ok()?;
        let key = d.get_str("key").unwrap_or_default().to_string();
        Some((updated_at, key))
    }))
}
