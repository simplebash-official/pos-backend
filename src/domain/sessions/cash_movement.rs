use super::cash_direction::CashDirection;
use super::cash_movement_type::CashMovementType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A recorded movement of cash into or out of the drawer.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashMovement {
    pub id: String,
    pub key: String,
    pub movement_number: String,
    pub movement_type: CashMovementType,
    pub direction: CashDirection,
    pub amount_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    pub terminal_id: String,
    pub category_key: String,
    /// Live name of the category, so a rename shows on old movements.
    pub category_name: String,
    /// A one-off label typed instead of saving a new category; the row then points at the type's "Other".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_label: Option<String>,
    /// `sup_…`, `emp_…` or `pye_…` key from the payee directory, when the payee is a saved one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    /// Who was paid, withdrew or handed over the cash, as written on the voucher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    /// First day of the period a rent, bill or salary covers (`YYYY-MM-DD`).
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
    /// Why a manager let a payout go over the cash in the drawer, or why a movement was reversed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_reason: Option<String>,
    /// Set on a reversal: the movement it cancels. A reversal moves the drawer the opposite way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverses_key: Option<String>,
    /// Set when a reversal of this movement exists. Worked out when read, never stored on this row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reversed_by_key: Option<String>,
    /// Until when the person who recorded this can undo it without a manager. Absent for a reversal,
    /// a no-sale open, or a movement that is already reversed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo_until: Option<DateTime<Utc>>,
    pub recorded_late: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_variance_session_key: Option<String>,
    /// The supplier receipt this movement was recorded with, when it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}
