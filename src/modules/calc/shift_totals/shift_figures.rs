use crate::domain::sessions::SessionStatus;

/// What the totals need from one shift.
#[derive(Debug)]
pub struct ShiftFigures {
    pub status: SessionStatus,
    pub cash_sales_cents: i64,
    pub cash_variance_cents: Option<i64>,
}
