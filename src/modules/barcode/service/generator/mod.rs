// The counter-backed generator: the single entry point other modules call
// to mint a new barcode (`inventory::service::product::create_product` is
// the first and only caller today, when a client requests auto-generation).

mod generate;
pub(crate) mod namespaces;
mod reserve_block;

pub(crate) use generate::generate;
pub(crate) use reserve_block::reserve_block;
