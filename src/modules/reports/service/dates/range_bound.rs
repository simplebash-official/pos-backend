/// Which end of a date range a bare `YYYY-MM-DD` stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeBound {
    /// The first instant of the day.
    Start,
    /// The exclusive upper bound: midnight at the start of the next day.
    End,
}
