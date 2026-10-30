//! The counter-backed generator: the single entry point other modules call
//! to mint a new barcode (`inventory::service::product::create_product` is
//! the first and only caller today, when a client requests auto-generation).

pub(crate) mod namespaces;
mod service_generator;

pub(crate) use service_generator::generate;
pub(crate) use service_generator::reserve_block;
