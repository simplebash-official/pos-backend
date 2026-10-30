use super::seed_summary::SeedSummary;
use crate::clients::db::Db;
use crate::core::error::AppError;

/// Runs all seeders sequentially to bootstrap a complete, realistic POS environment.
/// Compatible with both SQLite and MongoDB.
pub async fn seed_all(db: &Db) -> Result<SeedSummary, AppError> {
    let admin = super::admin::seed_admin(db, None, None).await?;
    let providers = super::providers::seed_providers(db).await?;
    let cash_categories = super::cash_categories::seed_cash_categories(db).await?;
    let suppliers = super::suppliers::seed_suppliers(db).await?;
    let customers = super::customers::seed_customers(db).await?;
    let inventory = super::inventory::seed_inventory(db).await?;
    let api_key = super::api_key::seed_api_key(db).await?;

    Ok(SeedSummary {
        admin,
        providers,
        cash_categories,
        suppliers,
        customers,
        inventory,
        api_key,
    })
}
