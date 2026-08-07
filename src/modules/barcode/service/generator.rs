// The counter-backed generator: the single entry point other modules call
// to mint a new barcode (`inventory::service::product::create_product` is
// the first and only caller today, when a client requests auto-generation).

use mongodb::Database;

use crate::{
    core::error::AppResult, modules::barcode::repository, modules::barcode::service::ean13,
};

/// Known barcode namespaces — avoids a magic string at every call site, the
/// same spirit as `core::constants::prefixes`.
pub(crate) mod namespaces {
    pub(crate) const PRODUCT: &str = "product";
}

/// Increments `namespace`'s counter and returns a freshly-encoded barcode.
/// This module has no opinion about what's being labeled — the caller
/// (e.g. `inventory`) owns uniqueness-against-its-own-collection checks and
/// any business rules about when generation should happen.
pub(crate) async fn generate(db: &Database, namespace: &str) -> AppResult<String> {
    let seq = repository::next_sequence(db, namespace).await?;
    ean13::encode(namespace, seq)
}
