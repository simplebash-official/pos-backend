use super::cash_movement_type::CashMovementType;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /billing/cash-movements`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCashMovementRequest {
    pub movement_type: CashMovementType,
    /// Zero for `no_sale`, greater than zero for every other type.
    pub amount_cents: i64,
    /// The open shift this belongs to. Absent only for cash taken out between shifts, which the
    /// drawer ledger carries into the next shift's opening count.
    #[serde(default)]
    pub session_key: Option<String>,
    pub terminal_id: String,
    pub category_key: String,
    #[serde(default)]
    pub custom_label: Option<String>,
    #[serde(default)]
    pub payee_key: Option<String>,
    #[serde(default)]
    pub payee_name: Option<String>,
    #[serde(default)]
    pub period_start: Option<String>,
    #[serde(default)]
    pub period_end: Option<String>,
    #[serde(default)]
    pub reference_no: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub manager_pin: Option<String>,
    /// Why a payout bigger than the cash in the drawer is allowed; required together with a manager PIN in that case.
    #[serde(default)]
    pub override_reason: Option<String>,
    /// Set to record a rent or bill payment for a month that is already paid.
    #[serde(default)]
    pub confirm_duplicate: bool,
    /// For a supplier payment: the supplier receipts (invoices) this payment pays, and how much
    /// of it each. What is not allocated stays on the supplier's account.
    #[serde(default)]
    pub settlements: Vec<crate::domain::purchase_receipts::ReceiptSettlementRequest>,
}
