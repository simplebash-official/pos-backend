use super::origin_field::ORIGIN_FIELD;
use mongodb::bson::Document;

/// Sets the origin on a document about to be inserted or used as a
/// replacement.
#[must_use]
pub fn stamp_document(origin: &str, mut document: Document) -> Document {
    document.insert(ORIGIN_FIELD, origin);
    document
}
