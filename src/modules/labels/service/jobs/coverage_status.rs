use super::last_label::LastLabel;

use crate::domain::labels::LabelCoverageStatus;

/// A product's label state: `None` when it was never labelled and has nothing
/// in stock. `received_since` counts units entered after the last label.
pub(super) fn coverage_status(
    stock_quantity: i64,
    last: Option<LastLabel>,
    received_since: i64,
) -> Option<LabelCoverageStatus> {
    match last {
        None if stock_quantity > 0 => Some(LabelCoverageStatus::Never),
        None => None,
        Some(_) if received_since > 0 => Some(LabelCoverageStatus::Needed),
        Some(_) => Some(LabelCoverageStatus::Labelled),
    }
}
