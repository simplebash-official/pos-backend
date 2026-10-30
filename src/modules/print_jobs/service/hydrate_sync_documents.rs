use crate::core::error::AppResult;

use crate::domain::print_jobs::PrintJob;

use crate::modules::print_jobs::model::PrintJobDocument;

/// Converts a page of raw `print_jobs` documents — as read by the sync
/// module's cursor scan — into the `PrintJob` shape the REST reads return.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<PrintJob>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<PrintJobDocument>(document)?.into_print_job())
        })
        .collect()
}
