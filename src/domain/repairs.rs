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
    pub id: String,
    pub key: String,
    pub ticket_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub customer_phone: String,
    pub device_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    pub issue_description: String,
    pub status: String,
    pub estimated_cost_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_cost_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_value: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

/// Body for `POST /repairs` and `PUT /repairs/{id}`. `ticketNumber` is never
/// client-supplied — reserved server-side on create only (see
/// `service::create_repair`). `customerName`/`customerPhone` are only used
/// as-is when `customerKey` is absent (a walk-in with no linked account);
/// when `customerKey` resolves to a real customer, `service::create_repair`
/// overrides both from that record instead of trusting the payload.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRepairRequest {
    #[serde(default)]
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub customer_phone: String,
    pub device_model: String,
    #[serde(default)]
    pub serial_number: Option<String>,
    pub issue_description: String,
    #[serde(default)]
    pub status: Option<String>,
    pub estimated_cost_cents: i64,
    #[serde(default)]
    pub material_cost_cents: Option<i64>,
    #[serde(default)]
    pub assigned_employee_id: Option<String>,
    #[serde(default)]
    pub assigned_employee_name: Option<String>,
    #[serde(default)]
    pub split_type: Option<String>,
    #[serde(default)]
    pub split_value: Option<f64>,
}

/// Body for `PATCH /repairs/{id}`. Every field optional so a client sends
/// only what changed. Same `customerKey`-overrides-`customerName`/
/// `customerPhone` rule as `CreateRepairRequest` applies here too (see
/// `service::update_repair`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRepairRequest {
    pub customer_key: Option<String>,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub device_model: Option<String>,
    pub serial_number: Option<String>,
    pub issue_description: Option<String>,
    pub status: Option<String>,
    pub estimated_cost_cents: Option<i64>,
    pub material_cost_cents: Option<i64>,
    pub assigned_employee_id: Option<String>,
    pub assigned_employee_name: Option<String>,
    pub split_type: Option<String>,
    pub split_value: Option<f64>,
}

/// Query params for `GET /repairs`. `search` matches ticket number,
/// customer name/phone, and device model; `status` filters to one exact
/// status value.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct RepairListQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairListResponse {
    pub repairs: Vec<Repair>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}
