/// What performing a write did (drives the push ack).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WriteOutcome {
    Written,
    /// Nothing to do (already in the requested state).
    Unchanged,
    /// The lifecycle merge refused the change.
    Rejected(&'static str),
}
