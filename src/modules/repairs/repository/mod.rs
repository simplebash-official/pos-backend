// Mongo access for the `repairs` collection only. Same never-interpret-a-
// miss-as-an-error convention as every other repository in this codebase —
// `service` decides what a missing row means.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};

use crate::{core::error::AppResult, modules::repairs::model::RepairDocument};

fn repairs(db: &Database) -> Collection<RepairDocument> {
    db.collection("repairs")
}

pub(crate) async fn find_by_id(db: &Database, id: ObjectId) -> AppResult<Option<RepairDocument>> {
    Ok(repairs(db)
        .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
        .await?)
}

pub(crate) async fn find_by_key(db: &Database, key: &str) -> AppResult<Option<RepairDocument>> {
    Ok(repairs(db)
        .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
        .await?)
}

pub(crate) async fn find_by_id_or_key(
    db: &Database,
    id_or_key: &str,
) -> AppResult<Option<RepairDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    find_by_key(db, id_or_key).await
}

pub(crate) async fn insert(
    db: &Database,
    mut document: RepairDocument,
) -> AppResult<RepairDocument> {
    let result = repairs(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

pub(crate) async fn update(
    db: &Database,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<RepairDocument>> {
    Ok(repairs(db)
        .find_one_and_update(
            doc! { "_id": id, "deleted_at": { "$exists": false } },
            doc! { "$set": set_doc, "$inc": { "version": 1 } },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

/// Narrow status-only mutator, looked up by `key` — used by
/// `service::mark_delivered`, the cross-module hook `billing`'s
/// complete-sale flow calls. Bypasses the general `update` set-doc builder
/// since this is the one place a ticket's status changes without a client
/// PATCH body.
pub(crate) async fn set_status_by_key(
    db: &Database,
    key: &str,
    status: &str,
) -> AppResult<Option<RepairDocument>> {
    Ok(repairs(db)
        .find_one_and_update(
            doc! { "key": key, "deleted_at": { "$exists": false } },
            doc! {
                "$set": { "status": status, "updated_at": BsonDateTime::now() },
                "$inc": { "version": 1 },
            },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn delete(
    db: &Database,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<RepairDocument>> {
    let now = BsonDateTime::now();
    let mut set_doc = doc! { "deleted_at": now, "updated_at": now };
    if let Some(device) = device_id {
        set_doc.insert("updated_by_device", device);
    }
    Ok(repairs(db)
        .find_one_and_update(
            doc! { "_id": id, "deleted_at": { "$exists": false } },
            doc! { "$set": set_doc, "$inc": { "version": 1 } },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn list(
    db: &Database,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<RepairDocument>, u64)> {
    let mut effective_filter = filter;
    if !effective_filter.contains_key("deleted_at") {
        effective_filter.insert("deleted_at", doc! { "$exists": false });
    }

    let total = repairs(db)
        .count_documents(effective_filter.clone())
        .await?;

    let mut cursor = repairs(db)
        .find(effective_filter)
        .sort(doc! { "updated_at": -1 })
        .skip(skip)
        .limit(limit as i64)
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok((items, total))
}
