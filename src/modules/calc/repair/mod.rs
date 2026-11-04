//! What a repair job charges and costs, for the repair form. Pure; the rule is
//! `pos_calc_core::repair`, the same one the repair save path uses.

pub use pos_calc_core::repair::{CalcRepairRequest, RepairTotals};
mod compute;

pub use compute::compute;
