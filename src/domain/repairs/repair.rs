use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A repair ticket as returned to API clients. `id` (the Mongo `ObjectId` as
/// a hex string) is the route/lookup key; `ticket_number` (`REP-000001`,
/// reserved via `modules::sequences`) is the human-facing number printed on
/// receipts and shown in `ServiceJobPickerModal` on the frontend.
///
/// `assigned_employee_id` is resolved server-side against `modules::employees`
/// on create/update (`service::resolve_assignment_employee_name`) — an
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
    /// Date the shop promised the job would be ready for the customer
    /// (YYYY-MM-DD). Optional — set when a hand-back date was agreed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promised_ready_at: Option<String>,
    /// Derived flag: the promised-ready date has passed and the job is still
    /// open (not delivered/cancelled). Computed fresh on every read, never
    /// stored — mirrors `billing::Invoice.is_overdue`.
    #[serde(default)]
    pub is_overdue: bool,
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
