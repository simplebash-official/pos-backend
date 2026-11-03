//! When a warranty starting on a day ends, for the product form, the till and the serial picker.
//! Pure; the rule is `pos_calc_core::warranty`.

pub use pos_calc_core::warranty::{CalcWarrantyReply, CalcWarrantyRequest};
mod compute;

pub use compute::compute;
