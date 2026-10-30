//! Orchestrates "give me the PDF for this entity" — check for an existing
//! rendering first (cache-by-`(entity_key, document_type)`), otherwise call
//! the document-server, save the bytes to disk, and record the metadata.
//! This is the only module allowed to call `clients::document_server`
//! directly (see `document_server::DocumentServerClient`'s module comment).

mod documents_service;
pub(crate) mod templates;
#[cfg(test)]
mod tests;

pub(crate) use documents_service::print;
pub(crate) use documents_service::print_design_status;
pub(crate) use documents_service::render_uncached;
