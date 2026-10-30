use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CashMovementBlocker {
    AmountMissing,
    CategoryMissing,
    /// A no-sale open: why the drawer is being opened.
    OpenReasonMissing,
    RentPayeeMissing,
    PeriodMissing,
    StaffMissing,
    SupplierMissing,
    OverAllocated,
    WhoTookCashMissing,
    PinMissing,
    PinNotVerified,
    OverrideReasonMissing,
    PeriodAlreadyPaid,
}
