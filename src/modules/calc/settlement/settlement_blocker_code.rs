use serde::Serialize;

use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SettlementBlockerCode {
    /// The bill is not owed any more (or is not this supplier's).
    BillNotOwed,
    /// An amount of zero or less against a bill.
    AmountMissing,
    /// More than the bill still owes.
    OverBill,
    /// The same bill listed twice.
    BillRepeated,
    /// More put against bills than is being paid.
    OverPayment,
}
