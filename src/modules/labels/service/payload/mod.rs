//! Builds the JSON handed to document-server's "Product Sticker Label" template
//! (slug `product-label`, see `clients::document_server`). The contract is that
//! template's `lbl_temp_*.schema.json`; `schema_drift_tests` checks this builder
//! against the real sidecar so the two cannot drift apart.

mod service_payload;
#[cfg(test)]
mod tests;

pub(crate) use service_payload::Sticker;
pub(crate) use service_payload::build_label_data;
