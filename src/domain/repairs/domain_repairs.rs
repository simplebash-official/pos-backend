use super::repair_item::{RepairItem, RepairItemInput, RepairKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Statuses that mean the job is finished and off the shop's plate, so a
/// passed promised-ready date no longer matters.
const CLOSED_JOB_STATUSES: [&str; 2] = ["delivered", "cancelled"];

/// Whether a repair/print job should be flagged overdue: still open and its
/// `promised_ready_at` date has passed. Pure function of the two persisted
/// fields, recomputed on every read (`RepairDocument::into_repair` /
/// `PrintJobDocument::into_print_job`) — mirrors `billing::compute_is_overdue`.
#[must_use]
pub fn compute_job_is_overdue(status: &str, promised_ready_at: Option<&str>) -> bool {
    if CLOSED_JOB_STATUSES.contains(&status) {
        return false;
    }
    let Some(promised) = promised_ready_at else {
        return false;
    };
    match chrono::NaiveDate::parse_from_str(promised, "%Y-%m-%d") {
        Ok(due) => due < Utc::now().date_naive(),
        Err(_) => false,
    }
}

/// Body for `POST /repairs` and `PUT /repairs/{id}`. `ticketNumber` is never
/// client-supplied — reserved server-side on create only (see
/// `service::create_repair`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRepairRequest {
    /// Customer contact information.
    pub customer: RepairCustomer,
    /// Device model/brand.
    pub device_model: String,
    /// Optional serial number or IMEI.
    pub serial_number: Option<String>,
    /// Reported issue or symptom description.
    pub issue_description: String,
    /// Optional date the job was promised ready (YYYY-MM-DD).
    pub promised_ready_at: Option<String>,
    /// Initial status (defaults to "received").
    pub status: Option<String>,
    /// The work on the job. May be empty while the device is still being looked at; a job
    /// needs at least one item before it is `ready` or `delivered` (see
    /// `service::validate_price_required_for_status`).
    pub items: Vec<RepairItemInput>,
    /// Optional technician assignment.
    pub assignment: Option<RepairAssignment>,
}

/// A repair ticket as returned to API clients. `id` (the Mongo `ObjectId` as
/// a hex string) is the route/lookup key; `ticket_number` (`REP-000001`,
/// reserved via `modules::sequences`) is the human-facing number printed on
/// receipts and shown in `ServiceJobPickerModal` on the frontend.
///
/// `assigned_employee_id` is resolved server-side against `modules::employees`
/// on create/update (`employees::service::resolve_assignment`) — an
/// unresolvable id 404s before any write, and `assigned_employee_name` is
/// always server-derived from it, never trusted from the client.
/// `split_type`/`split_value` stay opaque display fields; the actual
/// commission math is computed on demand by
/// `modules::reports::service::employee_earnings`, not stored here.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Repair {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key identifying this repair ticket (e.g. rep_...).
    pub key: String,
    /// Human-friendly ticket number (e.g. REP-000001).
    pub ticket_number: String,
    /// Key of the customer the ticket is for.
    pub customer_key: String,
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
    /// Date the shop promised the job would be ready for the customer
    /// (YYYY-MM-DD). Optional — set when a hand-back date was agreed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promised_ready_at: Option<String>,
    /// Derived flag: the promised-ready date has passed and the job is still
    /// open (not delivered/cancelled). Computed fresh on every read, never
    /// stored — mirrors `billing::Invoice.is_overdue`.
    pub is_overdue: bool,
    /// Lifecycle status ("received", "diagnosing", "in_repair", "ready", "delivered",
    /// "cancelled").
    pub status: String,
    /// Ordinary paid work, or free work redoing an earlier job under its warranty.
    pub kind: RepairKind,
    /// The work on the job, in order.
    pub items: Vec<RepairItem>,
    /// What the customer pays: every item's labour and part price, less the item discounts
    /// (`pos_calc_core::repair::repair_totals`). Stored with the items so reports can add it up.
    pub price_cents: i64,
    /// What the parts cost the shop, added up the same way.
    pub cost_cents: i64,
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
    pub version: i64,
    /// Soft deletion timestamp if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl Repair {
    /// All the item discounts together: what was taken off the price the items add up to.
    #[must_use]
    pub fn discount_cents(&self) -> i64 {
        self.items
            .iter()
            .filter_map(|item| item.discount.as_ref())
            .map(|discount| discount.amount_cents)
            .sum()
    }
}

/// The `assignment{}` sub-object of `CreateRepairRequest`/
/// `UpdateRepairRequest` — every field is optional, so the whole object is
/// typically omitted entirely on an unassigned ticket. See `Repair`'s doc
/// comment for how `assignedEmployeeId` is resolved server-side.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairAssignment {
    /// Staff ID of assigned technician.
    pub assigned_employee_id: Option<String>,
    /// Name of assigned technician.
    pub assigned_employee_name: Option<String>,
    /// Commission split method ("percentage" or "fixed").
    pub split_type: Option<String>,
    /// Commission rate or fixed amount.
    pub split_value: Option<f64>,
}

/// The `customer{}` sub-object of `CreateRepairRequest`/`UpdateRepairRequest`: the registered
/// customer the ticket is for. The ticket's customer name and phone are always read from that
/// customer's record (`service::create_repair`/`update_repair`), never from the request.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairCustomer {
    /// Key of the registered customer.
    pub customer_key: String,
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
    /// `shop_today_range()`. Any other value (or absence) means all time.
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
    /// Sum of `priceCents` for tickets created today.
    pub today_revenue_cents: i64,
    /// Count of ALL non-deleted tickets (any date) whose status is neither
    /// "delivered" nor "cancelled" — i.e. still open in the pipeline.
    pub pending_job_count: u64,
    /// `todayRevenueCents / todayJobCount`, rounded; 0 when no tickets were
    /// created today.
    pub avg_job_value_cents: i64,
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
    /// Updated promised-ready date (YYYY-MM-DD). Omit the key to leave it
    /// untouched; there is no way to clear it back to unset via this endpoint.
    pub promised_ready_at: Option<String>,
    /// Updated status.
    pub status: Option<String>,
    /// The job's whole list of items, replacing the stored one. Omit it to leave the items as
    /// they are. An item sent with the `key` of one already on the job keeps that key.
    pub items: Option<Vec<RepairItemInput>>,
    /// Updated technician assignment.
    pub assignment: Option<RepairAssignment>,
}
