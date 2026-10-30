use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How a receipt is paid at the moment it is received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaymentSource {
    /// Cash from the register drawer.
    Drawer,
    Bank,
    Card,
    Online,
    /// The owner paid with their own money: recorded as capital put into the drawer and then
    /// paid out, so the drawer count stays right.
    OwnerPocket,
}
