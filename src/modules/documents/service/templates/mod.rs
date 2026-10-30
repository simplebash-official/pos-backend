// Which document-server template prints a given document. The document-server
// publishes, per tenant, the latest published version of every design family
// with the kind it is for; a shop stores one chosen *family* per kind, and a
// single print can override it. This file owns the rules (all pure functions
// over the template list, so they are unit-tested without HTTP) plus thin
// async wrappers that fetch the list.
//
// Resolution at print time (there is deliberately no silent fallback):
//   1. an explicit `?template=<family>` on the request -> must be offered
//      for the kind, else 422 TEMPLATE_UNAVAILABLE
//   2. the shop's stored family for that kind          -> must still be
//      offered, else 422 TEMPLATE_UNAVAILABLE until the shop picks again
//   3. nothing chosen                                  -> the kind's built-in
//      default (the template the document-server marks `x-default`)

mod cache_key;
mod default_for;
mod design_settings;
mod effective_options;
mod find_family;
mod invalid_option;
mod list;
mod list_in;
mod merge_settings;
mod merge_settings_in;
mod option_values;
mod render_preview;
mod resolve;
mod resolve_in;
mod resolved_template;
mod selections;
#[cfg(test)]
mod tests;
mod unavailable;
mod validate_option_value;
mod with_options;

pub(crate) use cache_key::cache_key;
pub(crate) use design_settings::DesignSettings;

pub(crate) use list::list;

pub(crate) use merge_settings::merge_settings;

pub(crate) use render_preview::render_preview;
pub(crate) use resolve::resolve;

pub(crate) use resolved_template::ResolvedTemplate;

pub(crate) use with_options::with_options;
