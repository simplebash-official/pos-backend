use super::derive_code::derive_code;

/// The derived SKU prefix (e.g. "PHO-SCR") for a `category`/`subcategory`
/// name pair, without generating or consuming a number — used by the sync
/// module to discover which SKU block families a linked device needs to keep
/// pre-fetched (see `modules::sync::routes_local::sku_prefixes`).
pub(crate) fn sku_prefix(category: &str, subcategory: &str) -> String {
    format!("{}-{}", derive_code(category), derive_code(subcategory))
}
