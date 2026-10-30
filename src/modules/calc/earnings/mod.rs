//! What a repair or print job earns and what the assigned employee's commission comes to, for the
//! job forms and the employee screen's sample. Pure; the maths is `pos_calc_core`.

pub use pos_calc_core::earnings::{CalcEarningsReply, CalcEarningsRequest, SplitKind};
mod compute;
#[cfg(test)]
mod tests;

pub use compute::compute;
