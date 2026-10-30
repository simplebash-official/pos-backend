use super::session_status::SessionStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A register shift session record.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegisterSession {
    pub id: String,
    pub key: String,
    pub session_number: String,
    pub terminal_id: String,
    pub cashier_id: String,
    pub cashier_name_snapshot: String,
    pub status: SessionStatus,
    pub opened_at: DateTime<Utc>,
    pub opening_float_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_denominations: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_note: Option<String>,
    pub opening_discrepancy_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_discrepancy_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_approved_by_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_approved_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closing_cashier_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closing_cashier_name_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_cash_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counted_cash_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cash_variance_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closing_denominations: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closing_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closing_float_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cash_drop_to_safe_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variance_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variance_approved_by_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variance_approved_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_by_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_reason: Option<String>,
    pub total_cash_sales_cents: i64,
    pub total_card_sales_cents: i64,
    pub total_online_sales_cents: i64,
    pub total_credit_sales_cents: i64,
    pub total_cash_refunds_cents: i64,
    pub total_pay_in_cents: i64,
    pub total_pay_out_cents: i64,
    pub total_owner_withdrawals_cents: i64,
    pub total_cash_drops_cents: i64,
    pub has_late_movements: bool,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
