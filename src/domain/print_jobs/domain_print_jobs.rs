use super::{PrintJobItem, PrintJobItemInput, PrintJobKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Body for `POST /print-jobs`. `customer.customerName`/`customerPhone` are
/// only used as-is when `customer.customerKey` is absent (a walk-in with no
/// linked account); when it resolves to a real customer,
/// `service::create_print_job` overrides both from that record instead of
/// trusting the payload — same rule as `repairs::CreateRepairRequest`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePrintJobRequest {
    /// Customer contact information.
    pub customer: PrintJobCustomer,
    /// Category of print job.
    pub job_type: String,
    /// Quantity of copies/items.
    pub quantity: i32,
    /// Optional date the job was promised ready (YYYY-MM-DD).
    pub promised_ready_at: Option<String>,
    /// Initial status (defaults to "received").
    pub status: Option<String>,
    /// A staff note about the job; left out or blank for none.
    pub note: Option<String>,
    /// The job's lines: service fees and parts. Empty while the job is not priced yet.
    pub items: Vec<PrintJobItemInput>,
    /// Staff assignment details.
    pub assignment: Option<PrintJobAssignment>,
}

/// Print job domain model representing an active or completed print job.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJob {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key identifying this print job (e.g. prj_...).
    pub key: String,
    /// Human-friendly ticket number (e.g. PRN-000001).
    pub ticket_number: String,
    /// Key of the customer the job is for.
    pub customer_key: String,
    /// Customer name.
    pub customer_name: String,
    /// Customer contact phone.
    pub customer_phone: String,
    /// `"mug" | "t-shirt" | "handbill" | "banner" | "custom"` — validated
    /// against that fixed set in `service`, not modeled as a Rust enum so
    /// the frontend's `PrintJobType` union stays the single source of truth
    /// without a serde-rename mapping to keep in sync.
    pub job_type: String,
    /// Quantity of printed items.
    pub quantity: i32,
    /// Date the shop promised the job would be ready (YYYY-MM-DD), if agreed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promised_ready_at: Option<String>,
    /// Derived flag: promised-ready date has passed and the job is still open
    /// (not delivered/cancelled). Computed on every read, never stored.
    pub is_overdue: bool,
    /// Status ("received", "in_repair" (in production), "ready", "delivered", "cancelled").
    pub status: String,
    /// A staff note about the job. Never printed for the customer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Ordinary paid work, or free work redoing an earlier job under its warranty.
    pub kind: PrintJobKind,
    /// The job's lines, in order.
    pub items: Vec<PrintJobItem>,
    /// What the customer pays: every line's price less the line discounts
    /// (`pos_calc_core::print_job::print_job_totals`). Stored with the items so reports can add
    /// it up.
    pub price_cents: i64,
    /// What the parts cost the shop, added up the same way.
    pub cost_cents: i64,
    /// Staff ID of assigned worker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_id: Option<String>,
    /// Display name of assigned worker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_name: Option<String>,
    /// Commission split type ("percentage" or "fixed").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_type: Option<String>,
    /// Commission split value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_value: Option<f64>,
    /// Key of the invoice that billed this job. Absent until it is billed; set together with
    /// `billed_invoice_number`, and cleared when that invoice is voided. A billed job cannot be
    /// billed again and its items are locked, whether or not it has been handed back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billed_invoice_key: Option<String>,
    /// Number of the invoice that billed this job (e.g. INV-000034).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billed_invoice_number: Option<String>,
    /// Timestamp when job was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when job was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic concurrency version.
    pub version: i64,
    /// Soft deletion timestamp if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl PrintJob {
    /// All the line discounts together: what was taken off the price the lines add up to.
    #[must_use]
    pub fn discount_cents(&self) -> i64 {
        self.items
            .iter()
            .filter_map(|item| item.discount.as_ref())
            .map(|discount| discount.amount_cents)
            .sum()
    }
}

/// The `assignment{}` sub-object of `CreatePrintJobRequest`/
/// `UpdatePrintJobRequest` — same shape as `repairs::RepairAssignment`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobAssignment {
    /// User ID of employee assigned to job.
    pub assigned_employee_id: Option<String>,
    /// Name of employee assigned to job.
    pub assigned_employee_name: Option<String>,
    /// Commission split type ("percentage" or "fixed").
    pub split_type: Option<String>,
    /// Commission rate or fixed amount.
    pub split_value: Option<f64>,
}

/// The `customer{}` sub-object of `CreatePrintJobRequest`/`UpdatePrintJobRequest`: the registered
/// customer the job is for, same as `repairs::RepairCustomer`. Name and phone are read from that
/// customer's record.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobCustomer {
    /// Key of the registered customer.
    pub customer_key: String,
}

/// Query parameters for listing and filtering print jobs.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PrintJobListQuery {
    /// Keyword search matching ticket number or customer name/phone.
    pub search: Option<String>,
    /// Filter by status.
    pub status: Option<String>,
    /// Only `"today"` is meaningful — scopes to `created_at` within
    /// `shop_today_range()`. Any other value (or absence) means all time.
    pub date_preset: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Page items limit.
    pub limit: Option<u64>,
}

/// Paginated response payload containing a list of print jobs.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobListResponse {
    /// List of print jobs on current page.
    pub print_jobs: Vec<PrintJob>,
    /// Total count of matching print jobs.
    pub total: u64,
    /// Current page number.
    pub page: u64,
    /// Items limit per page.
    pub limit: u64,
    /// Total number of pages.
    pub total_pages: u64,
}

/// Response for `GET /print-jobs/stats` — the KPI cards on the frontend's
/// Print Jobs screen. Field names are byte-identical to `repairs::RepairStats`
/// (the frontend renders both through the same component) even though the
/// underlying collections differ.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobStats {
    /// Count of print jobs created today (UTC day boundary).
    pub today_job_count: u64,
    /// Sum of `priceCents` for jobs created today.
    pub today_revenue_cents: i64,
    /// Count of ALL non-deleted jobs (any date) whose status is neither
    /// "delivered" nor "cancelled" — i.e. still open in the pipeline.
    pub pending_job_count: u64,
    /// `todayRevenueCents / todayJobCount`, rounded; 0 when no jobs were
    /// created today.
    pub avg_job_value_cents: i64,
}

/// Body for `PATCH /print-jobs/{id}`. Same `customerKey`-overrides-
/// `customerName`/`customerPhone` rule as `CreatePrintJobRequest` applies
/// here too (see `service::update_print_job`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePrintJobRequest {
    /// Updated customer details.
    pub customer: Option<PrintJobCustomer>,
    /// Updated print job category.
    pub job_type: Option<String>,
    /// Updated quantity.
    pub quantity: Option<i32>,
    /// Updated promised-ready date (YYYY-MM-DD). Omit the key to leave it
    /// untouched; there is no way to clear it back to unset via this endpoint.
    pub promised_ready_at: Option<String>,
    /// Updated lifecycle status.
    pub status: Option<String>,
    /// Updated staff note. Omit it to keep the note; send a blank one to remove it.
    pub note: Option<String>,
    /// The job's whole list of lines, replacing the stored one. Omit it to leave the lines as
    /// they are. A line sent with the `key` of one already on the job keeps that key.
    pub items: Option<Vec<PrintJobItemInput>>,
    /// Updated staff assignment.
    pub assignment: Option<PrintJobAssignment>,
}
