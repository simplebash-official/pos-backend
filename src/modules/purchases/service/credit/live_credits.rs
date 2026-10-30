use crate::modules::purchases::model::SupplierCreditDocument;

use std::collections::HashSet;

/// The credits that still count: not voiding entries, and not voided by one.
pub(super) fn live_credits(rows: &[SupplierCreditDocument]) -> Vec<&SupplierCreditDocument> {
    let voided: HashSet<&str> = rows
        .iter()
        .filter_map(|r| r.reverses_key.as_deref())
        .collect();
    rows.iter()
        .filter(|r| r.reverses_key.is_none() && !voided.contains(r.key.as_str()))
        .collect()
}
