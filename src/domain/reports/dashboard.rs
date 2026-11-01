//! The dashboard pulse and reminders.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Response of `GET /reports/dashboard-pulse`: everything the dashboard groups and totals.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DashboardPulse {
    pub active_repairs_count: i64,
    pub ready_repairs_count: i64,
    /// Estimate plus parts of every repair waiting for pickup.
    pub uncollected_ready_value_cents: i64,
    pub active_print_jobs_count: i64,
    pub repair_pipeline: Vec<PulseStage>,
    pub print_pipeline: Vec<PulseStage>,
    pub technicians: Vec<PulseTechnician>,
    pub fast_movers: Vec<PulseFastMover>,
    pub low_stock: Vec<PulseLowStock>,
    pub ready_repairs: Vec<PulseReadyRepair>,
    pub recent_activity: Vec<PulseActivity>,
    pub pending_stock_quotations: Vec<PulseQuotation>,
}

/// One line of the live activity stream, newest first.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseActivity {
    /// `sale`, `repair` or `print`.
    pub kind: String,
    pub id: String,
    /// Invoice number or ticket number.
    pub reference: String,
    pub amount_cents: i64,
    pub created_at: DateTime<Utc>,
    pub payment_method: Option<String>,
    pub cashier_name: Option<String>,
    pub status: Option<String>,
    pub device_model: Option<String>,
    pub customer_name: Option<String>,
    pub job_type: Option<String>,
    pub quantity: Option<i64>,
}

/// A product selling today.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseFastMover {
    pub product_key: Option<String>,
    pub name: String,
    pub sold_today_units: i64,
    pub selling_price_cents: i64,
    /// The product's own figures; absent when the product no longer exists.
    pub category: Option<String>,
    pub remaining_stock: Option<i64>,
    pub min_threshold: Option<i64>,
}

/// A product at or below its low-stock line.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseLowStock {
    pub product_id: String,
    pub reference: String,
    pub name: String,
    pub stock_quantity: i64,
    /// The line it is measured against.
    pub threshold: i64,
    pub out_of_stock: bool,
    pub updated_at: DateTime<Utc>,
    /// Whole days since the product last changed.
    pub days_since_update: i64,
}

/// An active quotation with items requiring stock replenishment or ready to fill.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseQuotation {
    pub key: String,
    pub quotation_number: String,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub total_cents: i64,
    pub created_at: DateTime<Utc>,
    pub valid_until: String,
    pub items: Vec<PulseQuotationItem>,
    pub is_ready_to_fill: bool,
}

/// An item in a quotation that is tracked for stock fulfillment.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseQuotationItem {
    pub product_key: Option<String>,
    pub name: String,
    pub sku: Option<String>,
    pub quoted_quantity: i64,
    pub current_stock_quantity: i64,
    pub unit_price_cents: i64,
    pub has_sufficient_stock: bool,
}

/// A repair ready for pickup and still waiting.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseReadyRepair {
    pub id: String,
    pub ticket_number: String,
    pub device_model: String,
    pub customer_name: String,
    pub customer_phone: String,
    pub created_at: DateTime<Utc>,
    pub days_waiting: i64,
    /// `critical` after three days, otherwise `warning`.
    pub severity: String,
    /// What the customer will be asked for.
    pub amount_cents: i64,
}

// ---------------------------------------------------------------------------
// Dashboard pulse
// ---------------------------------------------------------------------------

/// How many jobs sit at one stage of a pipeline, and their share of everything tracked.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseStage {
    /// `received`, `diagnosing`, `in_repair`, `ready` or `delivered`.
    pub stage: String,
    pub count: i64,
    /// Whole percent of every job tracked in the pipeline, rounded half up.
    pub percent: i64,
}

/// One technician or printer operator's load.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseTechnician {
    pub employee_key: String,
    pub name: String,
    /// `technician` or `printer`.
    pub role: String,
    pub active_jobs_count: i64,
    pub completed_today_count: i64,
    /// Five active jobs is full (100).
    pub capacity_percent: i64,
    /// `available`, `busy` or `overloaded`.
    pub status: String,
    /// The first active job: its device and ticket.
    pub current_device_model: Option<String>,
    pub current_ticket_number: Option<String>,
}

/// One actionable reminder for the dashboard's attention list.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReminderEntry {
    pub kind: ReminderKind,
    /// Mongo hex id of the underlying invoice / job.
    pub id: String,
    /// Prefixed key (`inv_…` / `rep_…` / `prj_…`) of the underlying record.
    pub key: String,
    /// Human-facing number: invoice number or ticket number.
    pub reference_number: String,
    /// Short label — customer name for a credit reminder, device/job
    /// description for a job reminder.
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    /// Credit: the balance still owed. Job: the estimated cost (0 if unquoted).
    pub amount_cents: i64,
    /// The due / promised date (YYYY-MM-DD) — always present.
    pub due_date: String,
    /// Signed day delta: positive = overdue by N days, negative = due in N
    /// days, 0 = due today.
    pub days_from_due: i64,
    pub severity: String,
    /// Frontend deep link that opens the record with the right action.
    pub link_to: String,
}

/// What a single reminder is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReminderKind {
    /// A credit invoice whose payment due date has passed.
    CreditOverdue,
    /// A credit invoice due within the "due soon" window.
    CreditDueSoon,
    /// A repair or print job whose promised-ready date has passed.
    JobOverdue,
    /// A repair or print job promised ready within the "due soon" window.
    JobDueSoon,
}

/// Query parameters for the dashboard reminders feed (`GET /reports/reminders`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct RemindersQuery {
    /// How many days ahead counts as "due soon" (not yet overdue). Clamped
    /// to 1..=30; defaults to 7.
    pub due_within_days: Option<u32>,
    /// Max reminders returned in the merged list (headline counts still
    /// reflect the full set). Clamped to 1..=100; defaults to 20.
    pub limit: Option<u64>,
    /// Page number (1-indexed). Defaults to 1.
    pub page: Option<u64>,
}

/// The dashboard reminders feed: a merged, sorted list plus honest headline
/// counts over the full (pre-truncation) set.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemindersResponse {
    /// Overdue reminders first, then due-soon; each group ordered by date.
    pub reminders: Vec<ReminderEntry>,
    pub credit_overdue_count: u64,
    pub credit_overdue_amount_cents: i64,
    pub credit_due_soon_count: u64,
    pub credit_due_soon_amount_cents: i64,
    pub jobs_overdue_count: u64,
    pub jobs_due_soon_count: u64,
    pub due_within_days: u32,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}

/// Executive overview & KPI metrics for the Reports & Profit Intelligence dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportsDashboardResponse {
    /// Total net sales revenue in cents across all valid invoices in the period.
    pub total_revenue_cents: i64,
    /// Revenue generated from retail product sales in cents.
    pub retail_revenue_cents: i64,
    /// Revenue generated from repair services in cents.
    pub repair_revenue_cents: i64,
    /// Revenue generated from print jobs in cents.
    pub print_revenue_cents: i64,
    /// Count of completed invoices in the period.
    pub total_invoices_count: u64,
    /// Total discount amount granted in cents.
    pub total_discounts_cents: i64,
    /// Estimated cost of goods sold and service materials in cents.
    pub estimated_cogs_cents: i64,
    /// Estimated gross profit (total_revenue - estimated_cogs) in cents.
    pub estimated_gross_profit_cents: i64,
    /// Total employee profit-split commission payouts in cents.
    pub total_commissions_cents: i64,
    /// Net shop profit (gross_profit - total_commissions) in cents.
    pub net_shop_profit_cents: i64,
    /// Total uncollected credit balance across all pending/partially paid credit invoices (all-time).
    pub outstanding_credit_cents: i64,
    /// Start of the evaluated period (UTC).
    pub period_start: DateTime<Utc>,
    /// End of the evaluated period (UTC).
    pub period_end: DateTime<Utc>,
}
