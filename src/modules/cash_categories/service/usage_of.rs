use crate::modules::cash_categories::model::CashCategoryDocument;
use std::collections::HashMap;

/// Usage of a category including everything merged into it.
pub(super) fn usage_of(
    doc: &CashCategoryDocument,
    all: &[CashCategoryDocument],
    usage: &HashMap<String, i64>,
) -> i64 {
    let own = usage.get(&doc.key).copied().unwrap_or(0);
    let merged: i64 = all
        .iter()
        .filter(|c| c.merged_into_key.as_deref() == Some(doc.key.as_str()))
        .map(|c| usage.get(&c.key).copied().unwrap_or(0))
        .sum();
    own + merged
}
