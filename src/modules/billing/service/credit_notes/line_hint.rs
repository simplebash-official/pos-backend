/// Which of several lines of the same product a returned item means — a
/// product is on one line per price it was sold at.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LineHint<'a> {
    /// The price the returned unit was sold at (what a new return names).
    pub unit_price_cents: Option<i64>,
    /// The batches the returned units went back into (what a stored credit
    /// note carries); always a subset of the sold line's own batches.
    pub batch_keys: &'a [&'a str],
}
