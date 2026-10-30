#[derive(Debug)]
pub struct SeedCustomer {
    pub name: &'static str,
    pub contact_person: Option<&'static str>,
    pub primary_phone: &'static str,
    pub secondary_phone: Option<&'static str>,
    pub email: Option<&'static str>,
    pub address: Option<&'static str>,
    pub tags: Vec<&'static str>,
    pub notes: Option<&'static str>,
    pub outstanding_balance_cents: i64,
    pub total_purchases_cents: i64,
}
