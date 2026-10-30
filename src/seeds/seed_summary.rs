#[derive(Debug, Clone)]
pub struct SeedSummary {
    pub admin: super::admin::AdminSeedResult,
    pub providers: super::providers::ProviderSeedResult,
    pub cash_categories: super::cash_categories::CashCategorySeedResult,
    pub suppliers: super::suppliers::SupplierSeedResult,
    pub customers: super::customers::CustomerSeedResult,
    pub inventory: super::inventory::InventorySeedResult,
    pub api_key: super::api_key::ApiKeySeedResult,
}
