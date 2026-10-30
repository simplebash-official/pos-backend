use crate::domain::recurring_expenses::BillStatus;
use chrono::NaiveDate;

/// How a bill stands for a month, given today and what has been paid.
pub(super) fn status_for(
    today: NaiveDate,
    due: NaiveDate,
    remind_days_before: i64,
    amount_cents: i64,
    paid_cents: i64,
) -> BillStatus {
    if paid_cents >= amount_cents {
        BillStatus::Paid
    } else if today > due {
        BillStatus::Overdue
    } else if (due - today).num_days() <= remind_days_before {
        BillStatus::Due
    } else {
        BillStatus::Upcoming
    }
}
