//! Which document-server template prints a given document. The document-server
//! publishes, per tenant, the latest published version of every design family
//! with the kind it is for; a shop stores one chosen *family* per kind, and a
//! single print can override it. This file owns the rules (all pure functions
//! over the template list, so they are unit-tested without HTTP) plus thin
//! async wrappers that fetch the list.
//!
//! Resolution at print time (there is deliberately no silent fallback):
//!   1. an explicit `?template=<family>` on the request -> must be offered
//!      for the kind, else 422 TEMPLATE_UNAVAILABLE
//!   2. the shop's stored family for that kind          -> must still be
//!      offered, else 422 TEMPLATE_UNAVAILABLE until the shop picks again
//!   3. nothing chosen                                  -> the kind's built-in
//!      default (the template the document-server marks `x-default`)

mod service_templates;
#[cfg(test)]
mod tests;

pub(crate) use service_templates::DesignSettings;
pub(crate) use service_templates::cache_key;

pub(crate) use service_templates::list;

pub(crate) use service_templates::merge_settings;

pub(crate) use service_templates::render_preview;
pub(crate) use service_templates::resolve;

pub(crate) use service_templates::ResolvedTemplate;

pub(crate) use service_templates::with_options;
