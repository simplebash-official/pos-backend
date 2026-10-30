//! Mongo document shape for the `generated_documents` collection — metadata
//! only, never PDF bytes (those live on disk, see `service::get_or_render`).
//! No `into_*` conversion into a `domain` DTO exists because nothing outside
//! this module currently reads this shape as JSON; `billing`'s document
//! routes (Phase 4) stream the file straight off disk, not this record.

mod generated_document_document;

pub(crate) use generated_document_document::GeneratedDocumentDocument;
