// Orchestrates "give me the PDF for this entity" — check for an existing
// rendering first (cache-by-`(entity_key, document_type)`), otherwise call
// the document-server, save the bytes to disk, and record the metadata.
// This is the only module allowed to call `clients::document_server`
// directly (see `document_server::DocumentServerClient`'s module comment).

mod design_status;
mod get_or_render;
mod get_original;
mod is_safe_path_segment;
mod print;
mod print_design_status;
mod render_uncached;
pub(crate) mod templates;
#[cfg(test)]
mod tests;

pub(crate) use print::print;
pub(crate) use print_design_status::print_design_status;
pub(crate) use render_uncached::render_uncached;
