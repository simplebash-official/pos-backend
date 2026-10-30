use super::movement::to_movements;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::sessions::CashMovement;
use crate::modules::sessions::model::CashMovementDocument;
use mongodb::bson::Document;

/// Legacy `/sync/changes` items must match the REST DTO, which carries the live category name.
pub(crate) async fn hydrate_movement_sync_documents(
    db: &Db,
    documents: Vec<Document>,
) -> AppResult<Vec<CashMovement>> {
    let docs = documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<CashMovementDocument>(
                document,
            )?)
        })
        .collect::<AppResult<Vec<_>>>()?;
    to_movements(db, docs).await
}
