/// What a line earns at a selling price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineMargin {
    /// Selling price less the cost of one unit, to the nearest cent.
    pub profit_each_cents: i64,
    /// What the whole lot earns if every unit sells at the price. Negative is a loss.
    pub lot_profit_cents: i64,
    /// Profit as a share of the selling price in tenths of a percent (167 is 16.7%).
    pub margin_tenths: i64,
    pub below_cost: bool,
}
