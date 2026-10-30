use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::suppliers::SupplierStats;

use crate::modules::suppliers::repository;

/// Computes the KPI cards for the Suppliers screen.
pub async fn get_supplier_stats(db: &Db) -> AppResult<SupplierStats> {
    let (total_suppliers, direct_contacts_count) = repository::count_stats(db).await?;
    let supply_categories_count = repository::distinct_supplied_categories(db).await?.len() as u64;

    Ok(SupplierStats {
        total_suppliers,
        supply_categories_count,
        direct_contacts_count,
    })
}
