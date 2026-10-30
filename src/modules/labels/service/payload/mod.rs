// Builds the JSON handed to document-server's "Product Sticker Label" template
// (slug `product-label`, see `clients::document_server`). The contract is that
// template's `lbl_temp_*.schema.json`; `schema_drift_tests` checks this builder
// against the real sidecar so the two cannot drift apart.

mod build_label_data;
mod sticker;
#[cfg(test)]
mod tests;

pub(crate) use build_label_data::build_label_data;
pub(crate) use sticker::Sticker;
