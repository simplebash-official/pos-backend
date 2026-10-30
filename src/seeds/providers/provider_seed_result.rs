#[derive(Debug, Clone)]
pub struct ProviderSeedResult {
    pub categories_created: usize,
    pub subcategories_created: usize,
    pub categories_existing: usize,
}
