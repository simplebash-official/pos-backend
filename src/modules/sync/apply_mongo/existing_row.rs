use crate::core::sync_merge::RowMeta;

use mongodb::bson::Document;

/// An existing stored row: its merge metadata and the raw document.
#[derive(Debug)]
pub(crate) struct ExistingRow {
    pub meta: RowMeta,
    pub doc: Document,
}
