pub mod admin;
pub mod api_key;
pub mod customers;
pub mod inventory;
pub mod providers;
pub mod suppliers;

use crate::{clients::db::Db, core::error::AppError};

#[derive(Debug, Clone)]
pub struct SeedSummary {
    pub admin: admin::AdminSeedResult,
    pub providers: providers::ProviderSeedResult,
    pub suppliers: suppliers::SupplierSeedResult,
    pub customers: customers::CustomerSeedResult,
    pub inventory: inventory::InventorySeedResult,
    pub api_key: api_key::ApiKeySeedResult,
}

/// Runs all seeders sequentially to bootstrap a complete, realistic POS environment.
/// Compatible with both SQLite and MongoDB.
pub async fn seed_all(db: &Db) -> Result<SeedSummary, AppError> {
    let admin = admin::seed_admin(db, None, None).await?;
    let providers = providers::seed_providers(db).await?;
    let suppliers = suppliers::seed_suppliers(db).await?;
    let customers = customers::seed_customers(db).await?;
    let inventory = inventory::seed_inventory(db).await?;
    let api_key = api_key::seed_api_key(db).await?;

    Ok(SeedSummary {
        admin,
        providers,
        suppliers,
        customers,
        inventory,
        api_key,
    })
}
