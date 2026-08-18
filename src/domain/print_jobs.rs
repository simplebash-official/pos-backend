// Pure business types for the print-jobs feature — mirrors `domain::repairs`
// closely; the two modules differ only in their domain-specific fields
// (job type/quantity vs device model/issue description). See
// `domain::repairs`'s module comment for why `assigned_employee_*`/
// `split_*` are opaque display fields with no server-side commission math.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJob {
    pub id: String,
    pub key: String,
    pub ticket_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    /// `"mug" | "t-shirt" | "handbill" | "banner" | "custom"` — validated
    /// against that fixed set in `service`, not modeled as a Rust enum so
    /// the frontend's `PrintJobType` union stays the single source of truth
    /// without a serde-rename mapping to keep in sync.
    pub job_type: String,
    pub quantity: i32,
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

/// Body for `POST /print-jobs`. `customerName`/`customerPhone` are only
/// used as-is when `customerKey` is absent (a walk-in with no linked
/// account); when `customerKey` resolves to a real customer,
/// `service::create_print_job` overrides both from that record instead of
/// trusting the payload — same rule as `repairs::CreateRepairRequest`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePrintJobRequest {
    #[serde(default)]
    pub customer_key: Option<String>,
    pub customer_name: String,
    #[serde(default)]
    pub customer_phone: Option<String>,
    pub job_type: String,
    pub quantity: i32,
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

/// Body for `PATCH /print-jobs/{id}`. Same `customerKey`-overrides-
/// `customerName`/`customerPhone` rule as `CreatePrintJobRequest` applies
/// here too (see `service::update_print_job`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePrintJobRequest {
    pub customer_key: Option<String>,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub job_type: Option<String>,
    pub quantity: Option<i32>,
    pub status: Option<String>,
    pub estimated_cost_cents: Option<i64>,
    pub material_cost_cents: Option<i64>,
    pub assigned_employee_id: Option<String>,
    pub assigned_employee_name: Option<String>,
    pub split_type: Option<String>,
    pub split_value: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PrintJobListQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobListResponse {
    pub print_jobs: Vec<PrintJob>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}
