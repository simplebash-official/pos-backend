// The counter-backed generator: the single entry point other modules call
// to mint a new barcode (`inventory::service::product::create_product` is
// the first and only caller today, when a client requests auto-generation).

use crate::{
    clients::db::Db, core::error::AppResult, modules::barcode::repository,
    modules::barcode::service::ean13,
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
///
/// A linked device first draws from a block the cloud pre-reserved for this
/// namespace (see `reserve_block`/`sync::blocks::next_raw_number`), so two
/// offline devices never mint the same barcode. If the block is exhausted (or
/// none was ever reserved), it falls back to a per-device-tagged series
/// instead of the plain shared counter: an EAN-13 payload is 10 numeric
/// digits after the namespace prefix, so the top 3 are this device's stable
/// numeric tag (`sync::blocks::device_numeric_tag`) and the bottom 7 are a
/// counter private to that tag — two devices' independent fallback counters
/// then occupy disjoint numeric ranges and can't collide (barring a tag
/// collision among many simultaneously-exhausted devices, an accepted
/// narrower guarantee than invoice/SKU's text-tag fallback gets, since EAN-13
/// has no room for free text). An unlinked device (or one that was never
/// given a block) keeps today's behaviour exactly: the plain shared counter.
pub(crate) async fn generate(db: &Db, namespace: &str) -> AppResult<String> {
    if let Some(pool) = db.as_sqlite() {
        // `reserve_sequence`'s "barcode"/"barcodes" branch always reserves
        // under the single literal block name "barcode" (there is only one
        // namespace today, `namespaces::PRODUCT`), so that's the exact name
        // to look a stored block up under too — NOT a `"barcode:{namespace}"`
        // composite, which would never match what was actually stored.
        debug_assert_eq!(namespace, namespaces::PRODUCT, "add per-namespace block naming if a second barcode namespace is ever introduced");
        if let Some(seq) = crate::modules::sync::blocks::next_raw_number(pool, "barcode").await? {
            return ean13::encode(namespace, seq);
        }
        let linked: i64 = sqlx::query_scalar("SELECT linked FROM sync_state WHERE id = 1")
            .fetch_optional(pool)
            .await?
            .unwrap_or(0);
        if linked != 0 {
            let tag = crate::modules::sync::blocks::device_numeric_tag(pool).await?;
            let local = repository::next_sequence(db, &format!("{namespace}:fallback")).await?;
            return ean13::encode(namespace, tag * 10_000_000 + local);
        }
    }

    let seq = repository::next_sequence(db, namespace).await?;
    ean13::encode(namespace, seq)
}

/// Reserves a contiguous block of `block_size` barcode sequence numbers under
/// `namespace` for a linked device to draw individual barcodes from offline.
pub(crate) async fn reserve_block(db: &Db, namespace: &str, block_size: i64) -> AppResult<(i64, i64)> {
    repository::reserve_block(db, namespace, block_size).await
}
