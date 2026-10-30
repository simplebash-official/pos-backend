use std::collections::BTreeSet;

/// Entities whose derived fields must be recomputed after a batch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DerivedScope {
    pub product_ids: BTreeSet<String>,
    /// Stock batches whose `remaining_quantity` must be re-summed.
    pub batch_keys: BTreeSet<String>,
    pub customer_keys: BTreeSet<String>,
    pub invoice_keys: BTreeSet<String>,
}

impl DerivedScope {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.product_ids.is_empty()
            && self.batch_keys.is_empty()
            && self.customer_keys.is_empty()
            && self.invoice_keys.is_empty()
    }

    pub fn merge(&mut self, other: DerivedScope) {
        self.product_ids.extend(other.product_ids);
        self.batch_keys.extend(other.batch_keys);
        self.customer_keys.extend(other.customer_keys);
        self.invoice_keys.extend(other.invoice_keys);
    }
}
