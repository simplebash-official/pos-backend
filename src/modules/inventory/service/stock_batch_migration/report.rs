use super::summary::Summary;

use std::collections::BTreeMap;

/// A run's outcome, per shop (`tenant_id`; `SINGLE_SHOP` for documents that
/// carry none, i.e. a single-shop deployment).
#[derive(Debug, Default)]
pub struct Report {
    pub shops: BTreeMap<String, Summary>,
}

impl Report {
    /// Every shop's counts added together.
    #[must_use]
    pub fn total(&self) -> Summary {
        let mut total = Summary::default();
        for summary in self.shops.values() {
            total.add(summary);
        }
        total
    }

    pub(super) fn shop(&mut self, tenant: String) -> &mut Summary {
        self.shops.entry(tenant).or_default()
    }
}
