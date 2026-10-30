use chrono::{Datelike, NaiveDate};

pub(super) fn previous_month(first_day: NaiveDate) -> NaiveDate {
    first_day
        .pred_opt()
        .expect("a day before exists")
        .with_day(1)
        .expect("day 1 exists")
}
