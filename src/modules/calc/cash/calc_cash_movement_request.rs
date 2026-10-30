use crate::domain::sessions::CashMovementType;
use serde::Deserialize;
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// A drawer payment
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCashMovementRequest {
    pub movement_type: CashMovementType,
    pub amount_cents: i64,
    pub category_key: Option<String>,
    /// The open shift; absent for cash taken out between shifts.
    pub session_key: Option<String>,
    pub terminal_id: String,
    /// A saved payee (supplier, staff member, landlord...).
    pub payee_key: Option<String>,
    /// A name typed instead of picking a saved payee.
    pub payee_name: String,
    /// `YYYY-MM`, the month a rent, bill or salary covers.
    pub period: Option<String>,
    /// What a supplier payment puts against each bill.
    pub settlement_amounts_cents: Vec<i64>,
    pub pin_entered: bool,
    pub pin_verified: bool,
    pub override_reason_entered: bool,
    pub confirm_duplicate: bool,
}
