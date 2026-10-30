use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /suppliers/stats` — the KPI cards on the frontend's
/// Suppliers screen.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierStats {
    /// Count of non-deleted suppliers.
    pub total_suppliers: u64,
    /// Count of distinct tags across every supplier's `suppliedCategories`.
    pub supply_categories_count: u64,
    /// Count of suppliers with a non-empty `contactPerson`.
    pub direct_contacts_count: u64,
}
