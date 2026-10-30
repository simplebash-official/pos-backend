use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::seeds;

/// Loads the demo data. Suppliers, customers and the catalog do not depend on each other, so they
/// are seeded at the same time (on this task, so a tenant scope still applies); each one is mostly
/// waiting on the database, which is what made a web setup take minutes when done one by one.
pub(super) async fn seed_sample_data(db: &Db) -> AppResult<()> {
    let catalog = async {
        seeds::providers::seed_providers(db).await?;
        seeds::inventory::seed_inventory(db).await
    };
    tokio::try_join!(
        catalog,
        seeds::suppliers::seed_suppliers(db),
        seeds::customers::seed_customers(db),
    )?;
    Ok(())
}
