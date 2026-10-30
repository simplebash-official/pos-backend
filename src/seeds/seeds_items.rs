use crate::clients::db::Db;
use crate::core::config::SeedAdminConfig;
use crate::core::error::AppError;

/// Runs all seeders sequentially to bootstrap a complete, realistic POS environment.
/// Compatible with both SQLite and MongoDB.
pub async fn seed_all(db: &Db, admin: &SeedAdminConfig) -> Result<SeedSummary, AppError> {
    let admin_result = super::admin::seed_admin(db, admin).await?;
    let providers = super::providers::seed_providers(db).await?;
    let cash_categories = super::cash_categories::seed_cash_categories(db).await?;
    let suppliers = super::suppliers::seed_suppliers(db).await?;
    let customers = super::customers::seed_customers(db).await?;
    let inventory = super::inventory::seed_inventory(db).await?;
    let api_key = super::api_key::seed_api_key(db).await?;

    Ok(SeedSummary {
        admin: admin_result,
        providers,
        cash_categories,
        suppliers,
        customers,
        inventory,
        api_key,
    })
}

#[derive(Debug, Clone)]
/// Seed row for a summary.
pub struct SeedSummary {
    pub admin: super::admin::AdminSeedResult,
    pub providers: super::providers::ProviderSeedResult,
    pub cash_categories: super::cash_categories::CashCategorySeedResult,
    pub suppliers: super::suppliers::SupplierSeedResult,
    pub customers: super::customers::CustomerSeedResult,
    pub inventory: super::inventory::InventorySeedResult,
    pub api_key: super::api_key::ApiKeySeedResult,
}
