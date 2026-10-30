use super::cash_movement_blocker::CashMovementBlocker;

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCashMovementReply {
    pub takes_cash_out: bool,
    pub cash_in_drawer_cents: i64,
    /// The drawer after this payment.
    pub cash_after_cents: i64,
    /// The change to the drawer: positive adds cash, negative takes it out.
    pub drawer_delta_cents: i64,
    pub over_drawer: bool,
    pub pin_required: bool,
    /// The month a rent or bill covers already has a payment.
    pub period_already_paid: bool,
    /// What stops the payment being saved, in the order to say it; empty means it can go ahead.
    pub blockers: Vec<CashMovementBlocker>,
}
