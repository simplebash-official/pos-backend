// Label designs and label printing. A design is a shop's own layout (or one of
// the built-in starters); printing resolves the chosen products/serials into
// stickers server-side (barcode, name, SKU, category and price all come from
// the records, never from the client) and renders them through document-server
// as one PDF page per sticker. This module is the only caller of
// `documents::service::render_uncached` for labels.

mod built_in_conflict;
mod create_template;
mod delete_template;
mod ensure_name_free;
mod find_template;
mod has_price_element;
pub(crate) mod jobs;
mod list_templates;
pub(crate) mod payload;
mod preview_label;
mod render;
mod render_labels;
mod resolve_stickers;
pub(crate) mod starters;
mod template_not_found;
mod update_template;
pub(crate) mod validate;

pub(crate) use create_template::create_template;
pub(crate) use delete_template::delete_template;
use find_template::find_template;
use has_price_element::has_price_element;
pub(crate) use list_templates::list_templates;
pub(crate) use preview_label::preview_label;
use render::render;
pub(crate) use render_labels::render_labels;
use resolve_stickers::resolve_stickers;
pub(crate) use update_template::update_template;
