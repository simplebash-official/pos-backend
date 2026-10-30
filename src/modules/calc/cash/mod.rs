//! The cash screens' live figures: what a drawer payment does to the drawer and what stops it being
//! saved, what a counted pile of notes and coins comes to, and how a shift's count compares with
//! what the drawer should hold. The rules are `domain::sessions` (the same code the movement and
//! shift saves use); the handler reads the drawer, the category and the shop limit, this module
//! only combines them.

pub use pos_calc_core::cash::{CalcCashCountReply, CalcCashCountRequest, CashCountLine};
mod calc_cash;
#[cfg(test)]
mod tests;

pub use calc_cash::CalcCashMovementReply;
pub use calc_cash::CalcCashMovementRequest;
pub use calc_cash::CalcShiftCountReply;
pub use calc_cash::CalcShiftCountRequest;
pub use calc_cash::CashMovementBlocker;
pub use calc_cash::MovementContext;
pub use calc_cash::compute_count;
pub use calc_cash::compute_movement;
pub use calc_cash::compute_shift_count;
