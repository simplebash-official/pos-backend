//! Storage of generated documents.

mod documents_repository;

pub(crate) use documents_repository::find_latest_by_entity_and_type;
pub(crate) use documents_repository::find_latest_by_entity_and_variant;
pub(crate) use documents_repository::insert;
