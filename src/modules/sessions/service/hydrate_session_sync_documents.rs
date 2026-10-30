use crate::core::error::AppResult;
use crate::domain::sessions::RegisterSession;
use crate::modules::sessions::model::RegisterSessionDocument;
use mongodb::bson::Document;

pub(crate) fn hydrate_session_sync_documents(
    documents: Vec<Document>,
) -> AppResult<Vec<RegisterSession>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<RegisterSessionDocument>(document)?
                    .into_register_session(),
            )
        })
        .collect()
}
