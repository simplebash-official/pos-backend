use super::default_version::default_version;
use crate::core::constants::prefixes;
use crate::core::id::generate_id;
use crate::domain::sessions::{CashDirection, CashMovement, CashMovementType};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

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
    /// The supplier receipt this movement was recorded with (paid on delivery, or the own-pocket
    /// pair), when it was; later payments against a receipt do not carry it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_key: Option<String>,
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
    #[must_use]
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
                .then(|| {
                    self.created_at.to_chrono() + chrono::Duration::seconds(undo_window_seconds)
                }),
            reverses_key: self.reverses_key,
            reversed_by_key,
            recorded_late: self.recorded_late,
            original_variance_session_key: self.original_variance_session_key,
            receipt_key: self.receipt_key,
            version: self.version,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
