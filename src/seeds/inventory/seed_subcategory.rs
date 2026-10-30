use super::seed_product::SeedProduct;

#[derive(Debug)]
pub struct SeedSubcategory {
    pub name: &'static str,
    pub products: Vec<SeedProduct>,
}
