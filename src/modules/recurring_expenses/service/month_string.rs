use chrono::NaiveDate;

pub(super) fn month_string(first_day: NaiveDate) -> String {
    first_day.format("%Y-%m").to_string()
}
