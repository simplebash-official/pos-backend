use super::default_version::default_version;
use crate::core::constants::prefixes;
use crate::core::id::generate_id;
use crate::domain::sessions::{RegisterSession, SessionStatus};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterSessionDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub session_number: String,
    pub terminal_id: String,
    pub cashier_id: String,
    pub cashier_name_snapshot: String,
    pub status: SessionStatus,
    pub opened_at: BsonDateTime,
    pub opening_float_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_denominations: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_opening_float_cents: Option<i64>,
    #[serde(default)]
    pub opening_discrepancy_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_discrepancy_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_approved_by_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_approved_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<BsonDateTime>,
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
    pub reopened_at: Option<BsonDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_by_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_reason: Option<String>,
    #[serde(default)]
    pub total_cash_sales_cents: i64,
    #[serde(default)]
    pub total_card_sales_cents: i64,
    #[serde(default)]
    pub total_online_sales_cents: i64,
    #[serde(default)]
    pub total_credit_sales_cents: i64,
    #[serde(default)]
    pub total_cash_refunds_cents: i64,
    #[serde(default)]
    pub total_pay_in_cents: i64,
    #[serde(default)]
    pub total_pay_out_cents: i64,
    #[serde(default)]
    pub total_owner_withdrawals_cents: i64,
    #[serde(default)]
    pub total_cash_drops_cents: i64,
    #[serde(default)]
    pub has_late_movements: bool,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl RegisterSessionDocument {
    #[must_use]
    pub fn calculate_expected_cash(&self) -> i64 {
        self.opening_float_cents + self.total_cash_sales_cents + self.total_pay_in_cents
            - self.total_pay_out_cents
            - self.total_cash_refunds_cents
            - self.total_owner_withdrawals_cents
            - self.total_cash_drops_cents
    }

    #[must_use]
    pub fn into_register_session(self) -> RegisterSession {
        let expected_cash = self
            .expected_cash_cents
            .unwrap_or_else(|| self.calculate_expected_cash());
        let cash_variance = self
            .cash_variance_cents
            .or_else(|| self.counted_cash_cents.map(|c| c - expected_cash));
        let key = if self.key.is_empty() {
            generate_id(prefixes::REGISTER_SESSION)
        } else {
            self.key
        };
        RegisterSession {
            id: self
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(|| key.clone()),
            key,
            session_number: self.session_number,
            terminal_id: self.terminal_id,
            cashier_id: self.cashier_id,
            cashier_name_snapshot: self.cashier_name_snapshot,
            status: self.status,
            opened_at: self.opened_at.to_chrono(),
            opening_float_cents: self.opening_float_cents,
            opening_denominations: self.opening_denominations,
            opening_note: self.opening_note,
            opening_discrepancy_cents: self.opening_discrepancy_cents,
            opening_discrepancy_reason: self.opening_discrepancy_reason,
            opening_approved_by_id: self.opening_approved_by_id,
            opening_approved_by_name: self.opening_approved_by_name,
            closed_at: self.closed_at.map(|d| d.to_chrono()),
            closing_cashier_id: self.closing_cashier_id,
            closing_cashier_name_snapshot: self.closing_cashier_name_snapshot,
            expected_cash_cents: Some(expected_cash),
            counted_cash_cents: self.counted_cash_cents,
            cash_variance_cents: cash_variance,
            closing_denominations: self.closing_denominations,
            closing_note: self.closing_note,
            closing_float_cents: self.closing_float_cents,
            cash_drop_to_safe_cents: self.cash_drop_to_safe_cents,
            variance_reason: self.variance_reason,
            variance_approved_by_id: self.variance_approved_by_id,
            variance_approved_by_name: self.variance_approved_by_name,
            reopened_at: self.reopened_at.map(|d| d.to_chrono()),
            reopened_by_id: self.reopened_by_id,
            reopened_by_name: self.reopened_by_name,
            reopened_reason: self.reopened_reason,
            total_cash_sales_cents: self.total_cash_sales_cents,
            total_card_sales_cents: self.total_card_sales_cents,
            total_online_sales_cents: self.total_online_sales_cents,
            total_credit_sales_cents: self.total_credit_sales_cents,
            total_cash_refunds_cents: self.total_cash_refunds_cents,
            total_pay_in_cents: self.total_pay_in_cents,
            total_pay_out_cents: self.total_pay_out_cents,
            total_owner_withdrawals_cents: self.total_owner_withdrawals_cents,
            total_cash_drops_cents: self.total_cash_drops_cents,
            has_late_movements: self.has_late_movements,
            version: self.version,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
