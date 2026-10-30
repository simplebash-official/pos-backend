use chrono::NaiveDate;

pub(super) fn day(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}
