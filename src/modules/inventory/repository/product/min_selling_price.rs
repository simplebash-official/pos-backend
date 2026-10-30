/// Sort field standing for "a product's cheapest in-stock price". A product
/// stores no price (its batches do), so this is computed per row while
/// sorting: `min_price_stages` (Mongo) / `MIN_PRICE_SQL` (SQLite).
pub(crate) const MIN_SELLING_PRICE: &str = "min_selling_price_cents";
