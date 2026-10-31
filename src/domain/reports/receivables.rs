//! Receivables aging and outstanding invoices.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
/// Receivables falling in one age range.
pub struct AgingBucket {
    pub label: String,
    pub min_days: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_days: Option<i32>,
    pub invoice_count: u64,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
/// One customer's overdue receivables.
pub struct AgingDebtor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    pub outstanding_cents: i64,
    pub oldest_days: i32,
    pub invoice_count: u64,
}

/// One credit invoice with unpaid balance.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutstandingInvoiceEntry {
    pub id: String,
    pub key: String,
    pub invoice_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    pub total_cents: i64,
    pub amount_paid_cents: i64,
    pub balance_due_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    pub is_overdue: bool,
    pub created_at: DateTime<Utc>,
}

/// Query parameters for Outstanding Receivables report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct OutstandingReceivablesQuery {
    /// Optional customer name, phone, or invoice number search filter.
    pub search: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

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

// --- receivables aging -------------------------------------------------

/// Query for `GET /reports/analytics/receivables-aging`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ReceivablesAgingQuery {
    /// Snapshot date (YYYY-MM-DD), defaults to today.
    pub as_of: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceivablesAgingResponse {
    pub as_of: DateTime<Utc>,
    pub buckets: Vec<AgingBucket>,
    pub total_outstanding_cents: i64,
    pub total_invoices: u64,
    /// Customers with the largest 60+ day exposure (up to 10).
    pub top_debtors: Vec<AgingDebtor>,
}
