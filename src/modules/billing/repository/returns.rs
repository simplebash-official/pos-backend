// Mongo access for the `returns` collection. Functions here never interpret a
// missing document as an error — they return `Option`/`Vec`/counts straight
// from the driver and leave the not-found -> AppError translation to `service`.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc, oid::ObjectId},
};

use crate::{core::error::AppResult, modules::billing::model::ReturnDocument};

fn returns(db: &Database) -> Collection<ReturnDocument> {
    db.collection("returns")
}

/// Inserts a new return document and populates its auto-generated `_id`.
pub(crate) async fn insert_return(
    db: &Database,
    mut document: ReturnDocument,
) -> AppResult<ReturnDocument> {
    let result = returns(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

/// Look up a single return document by its Mongo ObjectId.
pub(crate) async fn find_return_by_id(
    db: &Database,
    id: ObjectId,
) -> AppResult<Option<ReturnDocument>> {
    Ok(returns(db).find_one(doc! { "_id": id }).await?)
}

/// Look up a single return document by its unique model key (`ret_...`).
pub(crate) async fn find_return_by_key(
    db: &Database,
    key: &str,
) -> AppResult<Option<ReturnDocument>> {
    Ok(returns(db).find_one(doc! { "key": key }).await?)
}

/// Look up a single return document by either its hex ObjectId or unique model key.
pub(crate) async fn find_return_by_id_or_key(
    db: &Database,
    id_or_key: &str,
) -> AppResult<Option<ReturnDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_return_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    find_return_by_key(db, id_or_key).await
}

/// Lists return documents matching a filter, sorted newest first with pagination.
pub(crate) async fn list_returns(
    db: &Database,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<ReturnDocument>, u64)> {
    let total = count_returns(db, filter.clone()).await?;

    let mut cursor = returns(db)
        .find(filter)
        .sort(doc! { "created_at": -1 })
        .skip(skip)
        .limit(limit as i64)
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok((items, total))
}

/// Returns the count of return documents matching the specified filter.
pub(crate) async fn count_returns(db: &Database, filter: Document) -> AppResult<u64> {
    Ok(returns(db).count_documents(filter).await?)
}
