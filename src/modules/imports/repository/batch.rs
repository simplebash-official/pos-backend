// Mongo access for the `import_batches` collection.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, doc},
    options::ReturnDocument,
};

use crate::{
    core::error::AppResult,
    modules::imports::model::{ImportBatchDocument, ImportRowErrorDocument},
};

fn import_batches(db: &Database) -> Collection<ImportBatchDocument> {
    db.collection("import_batches")
}

pub(crate) async fn insert_batch(
    db: &Database,
    mut document: ImportBatchDocument,
) -> AppResult<ImportBatchDocument> {
    let result = import_batches(db).insert_one(&document).await?;
    document.id = result.inserted_id.as_object_id();
    Ok(document)
}

pub(crate) async fn update_batch_result(
    db: &Database,
    key: &str,
    successful_rows: u64,
    failed_rows: u64,
    status: &str,
    errors: Vec<ImportRowErrorDocument>,
) -> AppResult<Option<ImportBatchDocument>> {
    let now = BsonDateTime::now();
    let bson_errors = errors
        .into_iter()
        .filter_map(|e| bson::serialize_to_document(&e).ok())
        .map(mongodb::bson::Bson::Document)
        .collect::<Vec<_>>();

    let update = doc! {
        "$set": {
            "successful_rows": successful_rows as i64,
            "failed_rows": failed_rows as i64,
            "status": status,
            "errors": bson_errors,
            "updated_at": now,
        }
    };

    let updated = import_batches(db)
        .find_one_and_update(doc! { "key": key }, update)
        .return_document(ReturnDocument::After)
        .await?;

    Ok(updated)
}

pub(crate) async fn find_batch_by_key(
    db: &Database,
    key: &str,
) -> AppResult<Option<ImportBatchDocument>> {
    Ok(import_batches(db).find_one(doc! { "key": key }).await?)
}

pub(crate) async fn list_batches(
    db: &Database,
    target: Option<&str>,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<ImportBatchDocument>, u64)> {
    let mut filter = doc! {};
    if let Some(t) = target.filter(|s| !s.is_empty()) {
        filter.insert("target", t);
    }

    let total = import_batches(db).count_documents(filter.clone()).await?;

    let mut cursor = import_batches(db)
        .find(filter)
        .sort(doc! { "created_at": -1 })
        .skip(skip)
        .limit(limit as i64)
        .await?;

    let mut items = Vec::new();
    while let Some(doc) = cursor.try_next().await? {
        items.push(doc);
    }

    Ok((items, total))
}
