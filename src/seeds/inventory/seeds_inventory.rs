#[derive(Debug, Clone)]
/// What seeding inventory created and skipped.
pub struct InventorySeedResult {
    pub categories_created: usize,
    pub subcategories_created: usize,
    pub products_created: usize,
    pub products_existing: usize,
}

#[derive(Debug)]
/// Seed row for a category.
pub struct SeedCategory {
    pub name: &'static str,
    pub icon: &'static str,
    pub color: &'static str,
    pub subcategories: Vec<SeedSubcategory>,
}

#[derive(Debug)]
/// Seed row for a product.
pub struct SeedProduct {
    pub name: &'static str,
    pub cost_price_cents: i64,
    pub selling_price_cents: i64,
    pub stock_quantity: i64,
    pub min_stock_threshold: i64,
    pub barcode: &'static str,
}

#[derive(Debug)]
/// Seed row for a subcategory.
pub struct SeedSubcategory {
    pub name: &'static str,
    pub products: Vec<SeedProduct>,
}
