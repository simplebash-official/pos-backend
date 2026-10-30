#[derive(Debug)]
pub struct SeedCategory {
    pub name: &'static str,
    pub icon: &'static str,
    pub color: &'static str,
    pub subcategory_names: Vec<&'static str>,
}
