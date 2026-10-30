use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcShiftCountReply {
    /// The count that was asked about.
    pub counted_cents: i64,
    /// What the drawer should hold.
    pub expected_cents: i64,
    /// Counted less expected; negative is a shortage.
    pub variance_cents: i64,
    /// Any difference needs an explanation.
    pub reason_required: bool,
    /// A manager PIN is needed: any difference at open, one beyond the tolerance at close.
    pub pin_required: bool,
    /// How far off a closing count can be before it needs a manager PIN.
    pub variance_tolerance_cents: i64,
    /// At close: the float that stays in the drawer, the one chosen or else the opening float.
    pub closing_float_cents: Option<i64>,
    /// At close: the opening float, or the whole count when that is less (the "keep opening float" button).
    pub keep_opening_float_cents: Option<i64>,
    /// At close: the cash taken to the safe, counted less the float left.
    pub safe_deposit_cents: Option<i64>,
}
