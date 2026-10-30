/// Sort presets the billing catalog understands. Kept in sync with the
/// frontend's `features/billing/lib/catalogSort.ts`; the backend only stores
/// and validates the id; the actual ordering is requested through the
/// products list's `sortBy`/`sortOrder`.
pub const CATALOG_SORT_IDS: &[&str] = &[
    "name_asc",
    "name_desc",
    "price_asc",
    "price_desc",
    "newest",
    "recent",
    "stock_desc",
    "stock_asc",
    "sku_asc",
];
