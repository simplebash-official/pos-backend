// Mongo document and persistent state models for register sessions,
// cash movements, and drawer ledger state.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::sessions::{
        CashDirection, CashMovement, CashMovementType, DrawerState, RegisterSession, SessionStatus,
    },
};


fn default_version() -> i64 {
    1
}

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
    pub fn calculate_expected_cash(&self) -> i64 {
        self.opening_float_cents
            + self.total_cash_sales_cents
            + self.total_pay_in_cents
            - self.total_pay_out_cents
            - self.total_cash_refunds_cents
            - self.total_owner_withdrawals_cents
            - self.total_cash_drops_cents
    }

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashMovementDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub movement_number: String,
    pub movement_type: CashMovementType,
    pub direction: CashDirection,
    pub amount_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    pub terminal_id: String,
    pub category_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub recorded_by_user_id: String,
    pub recorded_by_name_snapshot: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by_name_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverses_key: Option<String>,
    #[serde(default)]
    pub recorded_late: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_variance_session_key: Option<String>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl CashMovementDocument {
    /// `category_name` is the category's live name, resolved by the caller, so a rename shows on old rows;
    /// `reversed_by_key` is the reversal of this row, if one exists.
    pub fn into_cash_movement(
        self,
        category_name: String,
        reversed_by_key: Option<String>,
        undo_window_seconds: i64,
    ) -> CashMovement {
        let key = if self.key.is_empty() {
            generate_id(prefixes::CASH_MOVEMENT)
        } else {
            self.key
        };
        CashMovement {
            id: self
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(|| key.clone()),
            key,
            movement_number: self.movement_number,
            movement_type: self.movement_type,
            direction: self.direction,
            amount_cents: self.amount_cents,
            session_key: self.session_key,
            terminal_id: self.terminal_id,
            category_key: self.category_key,
            category_name,
            custom_label: self.custom_label,
            payee_key: self.payee_key,
            payee_name: self.payee_name,
            period_start: self.period_start,
            period_end: self.period_end,
            reference_no: self.reference_no,
            note: self.note,
            recorded_by_user_id: self.recorded_by_user_id,
            recorded_by_name_snapshot: self.recorded_by_name_snapshot,
            approved_by_user_id: self.approved_by_user_id,
            approved_by_name_snapshot: self.approved_by_name_snapshot,
            approval_reason: self.approval_reason,
            undo_until: (self.reverses_key.is_none()
                && reversed_by_key.is_none()
                && self.movement_type != CashMovementType::NoSale)
                .then(|| self.created_at.to_chrono() + chrono::Duration::seconds(undo_window_seconds)),
            reverses_key: self.reverses_key,
            reversed_by_key,
            recorded_late: self.recorded_late,
            original_variance_session_key: self.original_variance_session_key,
            version: self.version,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawerStateDocument {
    pub terminal_id: String,
    pub current_balance_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_session_key: Option<String>,
    pub last_closing_float_cents: i64,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl DrawerStateDocument {
    pub fn into_drawer_state(self) -> DrawerState {
        DrawerState {
            terminal_id: self.terminal_id,
            current_balance_cents: self.current_balance_cents,
            last_session_key: self.last_session_key,
            last_closing_float_cents: self.last_closing_float_cents,
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
