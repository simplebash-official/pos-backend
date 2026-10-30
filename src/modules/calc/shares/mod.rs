// How big each part is next to its whole, for the progress bars and split bars. Pure; the rule is
// `pos_calc_core::shares`.

pub use pos_calc_core::shares::{CalcSharesReply, CalcSharesRequest, ShareLine, SharePart};
mod compute;
#[cfg(test)]
mod tests;

pub use compute::compute;
