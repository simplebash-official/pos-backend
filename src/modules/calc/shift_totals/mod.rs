//! The figures over the shifts a list is showing: how many, how many are still running, the cash
//! sales together and the net cash difference of the closed ones. Pure; the handler reads the shifts
//! by key, so the totals can only be of real shifts and the list's own filters stay on the screen.

mod calc_shift_totals;
#[cfg(test)]
mod tests;

pub use calc_shift_totals::CalcShiftTotalsReply;
pub use calc_shift_totals::CalcShiftTotalsRequest;
pub use calc_shift_totals::ShiftFigures;
pub use calc_shift_totals::check;
pub use calc_shift_totals::compute;
