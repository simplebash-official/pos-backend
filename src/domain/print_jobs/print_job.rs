use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
