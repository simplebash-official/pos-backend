use chrono::{Datelike, NaiveDate};

/// The day a bill falls due in a month: the due day, or the month's last day when it has fewer days.
pub(super) fn due_date_in(first_day: NaiveDate, due_day: i64) -> NaiveDate {
    let next_month = if first_day.month() == 12 {
        NaiveDate::from_ymd_opt(first_day.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first_day.year(), first_day.month() + 1, 1)
    }
    .expect("a valid first of month");
    let last_day = i64::from(next_month.pred_opt().expect("a day before exists").day());
    first_day
        .with_day(due_day.min(last_day) as u32)
        .expect("a valid day of month")
}
