#[derive(Debug)]
pub struct SeedContactPerson {
    pub name: &'static str,
    pub phone_numbers: Vec<&'static str>,
}
