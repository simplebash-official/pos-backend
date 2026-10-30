use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /customers/stats` — the KPI cards on the frontend's
/// Customers screen.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerStats {
    /// Count of non-deleted customers.
    pub total_customers: u64,
    /// Sum of `outstandingBalanceCents` across all customers with a
    /// positive balance.
    pub total_balance_due_cents: i64,
    /// Count of customers with `outstandingBalanceCents > 0`.
    pub active_debtors_count: u64,
}
