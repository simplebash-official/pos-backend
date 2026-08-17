// Mongo access for the `generated_documents` collection only. Same
// never-interpret-a-miss-as-an-error convention as every other repository
// in this codebase — `service` decides what a missing row means.

use mongodb::{Collection, Database, bson::doc};

use crate::{core::error::AppResult, modules::documents::model::GeneratedDocumentDocument};

fn generated_documents(db: &Database) -> Collection<GeneratedDocumentDocument> {
    db.collection("generated_documents")
}

/// One row per `(entity_key, document_type)` in practice — `service`
/// enforces that by always checking here before rendering a new one — so
/// the most recent match is the only one that should ever exist, but sort
/// by `created_at` descending anyway to be defensive against any future
/// caller that re-renders without cleaning up the old row.
pub(crate) async fn find_latest_by_entity_and_type(
    db: &Database,
    entity_key: &str,
    document_type: &str,
) -> AppResult<Option<GeneratedDocumentDocument>> {
    Ok(generated_documents(db)
        .find_one(doc! { "entity_key": entity_key, "document_type": document_type })
        .sort(doc! { "created_at": -1 })
        .await?)
}

pub(crate) async fn insert(
    db: &Database,
    mut document: GeneratedDocumentDocument,
) -> AppResult<GeneratedDocumentDocument> {
    let result = generated_documents(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}
