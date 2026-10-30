#[derive(Debug)]
pub struct SeedProduct {
    pub name: &'static str,
    pub cost_price_cents: i64,
    pub selling_price_cents: i64,
    pub stock_quantity: i64,
    pub min_stock_threshold: i64,
    pub barcode: &'static str,
}
