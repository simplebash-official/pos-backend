//! Label designs and label printing. A design is a shop's own layout (or one of
//! the built-in starters); printing resolves the chosen products/serials into
//! stickers server-side (barcode, name, SKU, category and price all come from
//! the records, never from the client) and renders them through document-server
//! as one PDF page per sticker. This module is the only caller of
//! `documents::service::render_uncached` for labels.

pub(crate) mod jobs;
mod labels_service;
pub(crate) mod payload;
pub(crate) mod starters;
pub(crate) mod validate;

pub(crate) use labels_service::create_template;
pub(crate) use labels_service::delete_template;
use labels_service::find_template;
use labels_service::has_price_element;
pub(crate) use labels_service::list_templates;
pub(crate) use labels_service::preview_label;
use labels_service::render;
pub(crate) use labels_service::render_labels;
use labels_service::resolve_stickers;
pub(crate) use labels_service::update_template;
