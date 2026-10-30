/// What one run changed (or, in a dry run, would change) for one shop.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Summary {
    /// Products whose price fields are (would be) moved into a batch.
    pub products: u64,
    /// Opening batches created (would be created).
    pub batches: u64,
    pub movements: u64,
    pub serials: u64,
    pub purchases: u64,
    pub invoices: u64,
    pub credit_notes: u64,
    /// Products with a selling price of zero or less: their opening batch
    /// sells at that price until someone re-prices it.
    pub zero_price_products: u64,
    /// Products without a `key` or `_id`, which cannot be migrated and are skipped.
    pub skipped_products: u64,
    /// Old sale/return product lines left without a batch allocation because
    /// they recorded no cost and their product is not being migrated.
    pub lines_without_cost: u64,
}

impl Summary {
    pub(super) fn add(&mut self, other: &Summary) {
        self.products += other.products;
        self.batches += other.batches;
        self.movements += other.movements;
        self.serials += other.serials;
        self.purchases += other.purchases;
        self.invoices += other.invoices;
        self.credit_notes += other.credit_notes;
        self.zero_price_products += other.zero_price_products;
        self.skipped_products += other.skipped_products;
        self.lines_without_cost += other.lines_without_cost;
    }
}
