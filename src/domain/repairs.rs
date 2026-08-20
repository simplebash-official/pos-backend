// Pure business types for the repairs feature — no I/O, no Mongo/Axum types
// beyond serde/utoipa derives. Mongo document shapes live in
// `modules::repairs::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// A repair ticket as returned to API clients. `id` (the Mongo `ObjectId` as
/// a hex string) is the route/lookup key; `ticket_number` (`REP-000001`,
/// reserved via `modules::sequences`) is the human-facing number printed on
/// receipts and shown in `ServiceJobPickerModal` on the frontend.
///
/// `assigned_employee_id`/`name`, `split_type`/`split_value` are stored as
/// opaque display fields only — no `employees` backend module exists yet, so
/// no commission math happens here. The frontend still computes/records
/// technician earnings against its own (still-mocked) employees store after
/// a status update succeeds; see the migration plan's employees/commission
/// scope note.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Repair {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key identifying this repair ticket (e.g. rep_...).
    pub key: String,
    /// Human-friendly ticket number (e.g. REP-000001).
    pub ticket_number: String,
    /// Key of linked customer, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Customer name.
    pub customer_name: String,
    /// Customer phone number.
    pub customer_phone: String,
    /// Device model/brand.
    pub device_model: String,
    /// Serial number or IMEI of the device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    /// Diagnosis or reported issue description.
    pub issue_description: String,
    /// Lifecycle status ("received", "in_progress", "completed", "delivered", "cancelled").
    pub status: String,
    /// Quoted or final repair price in cents. Absent until a technician has
    /// diagnosed the device and quoted a price; see `service::
    /// validate_price_required_for_status` for which statuses require it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost_cents: Option<i64>,
    /// Direct cost of parts/materials in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_cost_cents: Option<i64>,
    /// Staff ID of assigned technician.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_id: Option<String>,
    /// Display name of assigned technician.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_name: Option<String>,
    /// Commission split calculation type ("percentage" or "fixed").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_type: Option<String>,
    /// Commission split value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_value: Option<f64>,
    /// Timestamp when ticket was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when ticket was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version number.
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

/// The `customer{}` sub-object of `CreateRepairRequest`/`UpdateRepairRequest`.
/// `customerName`/`customerPhone` are only used as-is when `customerKey` is
/// absent (a walk-in with no linked account); when `customerKey` resolves
/// to a real customer, `service::create_repair`/`update_repair` overrides
/// both from that record instead of trusting the payload. `customerName`/
/// `customerPhone` are typed `Option` here (rather than required strings)
/// so the same struct serves both create and update — `service::
/// create_repair`'s `validate_required_fields` is what actually enforces
/// they're non-empty on create when there's no `customerKey` to resolve
/// from instead.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairCustomer {
    /// Key of registered customer.
    #[serde(default)]
    pub customer_key: Option<String>,
    /// Walk-in customer name.
    #[serde(default)]
    pub customer_name: Option<String>,
    /// Walk-in customer phone number.
    #[serde(default)]
    pub customer_phone: Option<String>,
}

/// The `assignment{}` sub-object of `CreateRepairRequest`/
/// `UpdateRepairRequest` — every field is optional at every layer (no
/// `employees` backend module exists yet, see `Repair`'s doc comment), so
/// the whole object is typically omitted entirely on an unassigned ticket.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairAssignment {
    /// Staff ID of assigned technician.
    #[serde(default)]
    pub assigned_employee_id: Option<String>,
    /// Name of assigned technician.
    #[serde(default)]
    pub assigned_employee_name: Option<String>,
    /// Commission split method ("percentage" or "fixed").
    #[serde(default)]
    pub split_type: Option<String>,
    /// Commission rate or fixed amount.
    #[serde(default)]
    pub split_value: Option<f64>,
}

/// Body for `POST /repairs` and `PUT /repairs/{id}`. `ticketNumber` is never
/// client-supplied — reserved server-side on create only (see
/// `service::create_repair`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRepairRequest {
    /// Customer contact information.
    #[serde(default)]
    pub customer: RepairCustomer,
    /// Device model/brand.
    pub device_model: String,
    /// Optional serial number or IMEI.
    #[serde(default)]
    pub serial_number: Option<String>,
    /// Reported issue or symptom description.
    pub issue_description: String,
    /// Initial status (defaults to "received").
    #[serde(default)]
    pub status: Option<String>,
    /// Estimated charge to customer in cents. Optional: a ticket can be
    /// created before a technician has diagnosed the device and quoted a
    /// price; see `service::validate_price_required_for_status`.
    #[serde(default)]
    pub estimated_cost_cents: Option<i64>,
    /// Optional cost of parts/materials in cents.
    #[serde(default)]
    pub material_cost_cents: Option<i64>,
    /// Optional technician assignment.
    #[serde(default)]
    pub assignment: Option<RepairAssignment>,
}

/// Body for `PATCH /repairs/{id}`. Every field optional so a client sends
/// only what changed. Same `customerKey`-overrides-`customerName`/
/// `customerPhone` rule as `CreateRepairRequest` applies here too (see
/// `service::update_repair`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRepairRequest {
    /// Updated customer details.
    pub customer: Option<RepairCustomer>,
    /// Updated device model.
    pub device_model: Option<String>,
    /// Updated serial number or IMEI.
    pub serial_number: Option<String>,
    /// Updated issue description.
    pub issue_description: Option<String>,
    /// Updated status.
    pub status: Option<String>,
    /// Updated customer price estimate in cents. Omit the key to leave the
    /// existing price untouched; there is no way to explicitly clear a price
    /// back to unset via this endpoint; nothing in the workflow needs that.
    pub estimated_cost_cents: Option<i64>,
    /// Updated material cost in cents.
    pub material_cost_cents: Option<i64>,
    /// Updated technician assignment.
    pub assignment: Option<RepairAssignment>,
}

/// Query params for `GET /repairs`. `search` matches ticket number,
/// customer name/phone, and device model; `status` filters to one exact
/// status value.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct RepairListQuery {
    /// Search term matching ticket number, customer name/phone, or model.
    pub search: Option<String>,
    /// Filter by status.
    pub status: Option<String>,
    /// Only `"today"` is meaningful — scopes to `created_at` within
    /// `today_utc_range()`. Any other value (or absence) means all time.
    pub date_preset: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page limit.
    pub limit: Option<u64>,
}

/// Paginated response payload containing list of repair tickets.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairListResponse {
    /// List of repair tickets on current page.
    pub repairs: Vec<Repair>,
    /// Total count of matching repair tickets.
    pub total: u64,
    /// Current page number.
    pub page: u64,
    /// Items limit per page.
    pub limit: u64,
    /// Total number of pages.
    pub total_pages: u64,
}

/// Response for `GET /repairs/stats` — the KPI cards on the frontend's
/// Repair Jobs screen. Field names are byte-identical to `PrintJobStats`
/// (the frontend renders both through the same component) even though the
/// underlying collections differ.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairStats {
    /// Count of repair tickets created today (UTC day boundary).
    pub today_job_count: u64,
    /// Sum of `estimatedCostCents` (treated as 0 where unset/unquoted) for
    /// tickets created today.
    pub today_revenue_cents: i64,
    /// Count of ALL non-deleted tickets (any date) whose status is neither
    /// "delivered" nor "cancelled" — i.e. still open in the pipeline.
    pub pending_job_count: u64,
    /// `todayRevenueCents / todayJobCount`, rounded; 0 when no tickets were
    /// created today.
    pub avg_job_value_cents: i64,
}
