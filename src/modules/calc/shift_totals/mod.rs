// The figures over the shifts a list is showing: how many, how many are still running, the cash
// sales together and the net cash difference of the closed ones. Pure; the handler reads the shifts
// by key, so the totals can only be of real shifts and the list's own filters stay on the screen.

mod calc_shift_totals_reply;
mod calc_shift_totals_request;
mod check;
mod compute;
mod shift_figures;
#[cfg(test)]
mod tests;

pub use calc_shift_totals_reply::CalcShiftTotalsReply;
pub use calc_shift_totals_request::CalcShiftTotalsRequest;
pub use check::check;
pub use compute::compute;
pub use shift_figures::ShiftFigures;
