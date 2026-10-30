#[derive(Debug, Clone)]
pub struct InventorySeedResult {
    pub categories_created: usize,
    pub subcategories_created: usize,
    pub products_created: usize,
    pub products_existing: usize,
}
