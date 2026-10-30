use super::seed_contact_person::SeedContactPerson;

#[derive(Debug)]
pub struct SeedSupplier {
    pub name: &'static str,
    pub contact_person: &'static str,
    pub primary_phone: &'static str,
    pub secondary_phone: Option<&'static str>,
    pub contact_persons: Vec<SeedContactPerson>,
    pub address: &'static str,
    pub supplied_categories: Vec<&'static str>,
    pub email: Option<&'static str>,
    pub notes: Option<&'static str>,
}
