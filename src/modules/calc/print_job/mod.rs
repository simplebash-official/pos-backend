//! What a print job charges and costs, for the print form. Pure; the rule is
//! `pos_calc_core::print_job`, the same one the print job save path uses.

pub use pos_calc_core::print_job::{CalcPrintJobRequest, PrintJobPreview};
mod compute;

pub use compute::compute;
