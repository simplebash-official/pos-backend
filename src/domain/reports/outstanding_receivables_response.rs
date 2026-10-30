use super::outstanding_invoice_entry::OutstandingInvoiceEntry;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Outstanding receivables overview and paginated unpaid invoices.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutstandingReceivablesResponse {
    pub total_outstanding_cents: i64,
    pub total_credit_invoices_count: u64,
    pub overdue_invoices_count: u64,
    pub overdue_amount_cents: i64,
    pub invoices: Vec<OutstandingInvoiceEntry>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}
