use super::seed_subcategory::SeedSubcategory;

#[derive(Debug)]
pub struct SeedCategory {
    pub name: &'static str,
    pub icon: &'static str,
    pub color: &'static str,
    pub subcategories: Vec<SeedSubcategory>,
}
