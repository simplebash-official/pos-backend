// The cash screens' live figures: what a drawer payment does to the drawer and what stops it being
// saved, what a counted pile of notes and coins comes to, and how a shift's count compares with
// what the drawer should hold. The rules are `domain::sessions` (the same code the movement and
// shift saves use); the handler reads the drawer, the category and the shop limit, this module
// only combines them.

pub use pos_calc_core::cash::{CalcCashCountReply, CalcCashCountRequest, CashCountLine};
mod calc_cash_movement_reply;
mod calc_cash_movement_request;
mod calc_shift_count_reply;
mod calc_shift_count_request;
mod cash_movement_blocker;
mod check_cents;
mod compute_count;
mod compute_movement;
mod compute_shift_count;
mod movement_context;
#[cfg(test)]
mod tests;

pub use calc_cash_movement_reply::CalcCashMovementReply;
pub use calc_cash_movement_request::CalcCashMovementRequest;
pub use calc_shift_count_reply::CalcShiftCountReply;
pub use calc_shift_count_request::CalcShiftCountRequest;
pub use cash_movement_blocker::CashMovementBlocker;
pub use compute_count::compute_count;
pub use compute_movement::compute_movement;
pub use compute_shift_count::compute_shift_count;
pub use movement_context::MovementContext;
