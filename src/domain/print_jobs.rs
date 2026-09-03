// Pure business types for the print-jobs feature — mirrors `domain::repairs`
// closely; the two modules differ only in their domain-specific fields
// (job type/quantity vs device model/issue description). See
// `domain::repairs`'s module comment for how `assigned_employee_id` is
// resolved server-side against `modules::employees` and why `split_*`
// stay opaque display fields (commission math is computed on demand by
// `modules::reports::service::employee_earnings`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

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
    /// Key of linked customer, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Customer name.
    pub customer_name: String,
    /// Customer contact phone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
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
    #[serde(default)]
    pub is_overdue: bool,
    /// Status ("received", "in_progress", "completed", "delivered", "cancelled").
    pub status: String,
    /// Quoted or estimated price in cents.
    pub estimated_cost_cents: i64,
    /// Direct materials cost in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_cost_cents: Option<i64>,
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
    /// Timestamp when job was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when job was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic concurrency version.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Soft deletion timestamp if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

/// The `customer{}` sub-object of `CreatePrintJobRequest`/
/// `UpdatePrintJobRequest` — same shape and override rule as
/// `repairs::RepairCustomer`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobCustomer {
    /// Registered customer key.
    #[serde(default)]
    pub customer_key: Option<String>,
    /// Walk-in customer name.
    #[serde(default)]
    pub customer_name: Option<String>,
    /// Walk-in customer phone number.
    #[serde(default)]
    pub customer_phone: Option<String>,
}

/// The `assignment{}` sub-object of `CreatePrintJobRequest`/
/// `UpdatePrintJobRequest` — same shape as `repairs::RepairAssignment`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobAssignment {
    /// User ID of employee assigned to job.
    #[serde(default)]
    pub assigned_employee_id: Option<String>,
    /// Name of employee assigned to job.
    #[serde(default)]
    pub assigned_employee_name: Option<String>,
    /// Commission split type ("percentage" or "fixed").
    #[serde(default)]
    pub split_type: Option<String>,
    /// Commission rate or fixed amount.
    #[serde(default)]
    pub split_value: Option<f64>,
}

/// Body for `POST /print-jobs`. `customer.customerName`/`customerPhone` are
/// only used as-is when `customer.customerKey` is absent (a walk-in with no
/// linked account); when it resolves to a real customer,
/// `service::create_print_job` overrides both from that record instead of
/// trusting the payload — same rule as `repairs::CreateRepairRequest`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePrintJobRequest {
    /// Customer contact information.
    #[serde(default)]
    pub customer: PrintJobCustomer,
    /// Category of print job.
    pub job_type: String,
    /// Quantity of copies/items.
    pub quantity: i32,
    /// Optional date the job was promised ready (YYYY-MM-DD).
    #[serde(default)]
    pub promised_ready_at: Option<String>,
    /// Initial status (defaults to "received").
    #[serde(default)]
    pub status: Option<String>,
    /// Estimated total customer cost in cents.
    pub estimated_cost_cents: i64,
    /// Optional material cost in cents.
    #[serde(default)]
    pub material_cost_cents: Option<i64>,
    /// Staff assignment details.
    #[serde(default)]
    pub assignment: Option<PrintJobAssignment>,
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
    /// Updated customer charge estimate in cents.
    pub estimated_cost_cents: Option<i64>,
    /// Updated material cost in cents.
    pub material_cost_cents: Option<i64>,
    /// Updated staff assignment.
    pub assignment: Option<PrintJobAssignment>,
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
    /// `today_utc_range()`. Any other value (or absence) means all time.
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
    /// Sum of `estimatedCostCents` for jobs created today.
    pub today_revenue_cents: i64,
    /// Count of ALL non-deleted jobs (any date) whose status is neither
    /// "delivered" nor "cancelled" — i.e. still open in the pipeline.
    pub pending_job_count: u64,
    /// `todayRevenueCents / todayJobCount`, rounded; 0 when no jobs were
    /// created today.
    pub avg_job_value_cents: i64,
}
