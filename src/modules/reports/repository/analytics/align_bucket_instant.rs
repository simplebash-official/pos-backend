use crate::domain::reports::Granularity;
use chrono::{DateTime, Utc};

pub(super) fn align_bucket_instant(instant: DateTime<Utc>, gran: Granularity) -> DateTime<Utc> {
    use chrono::{Datelike, Duration, FixedOffset, NaiveDate};
    let tz = FixedOffset::east_opt(5 * 3600 + 30 * 60).expect("+05:30 is a valid offset");
    let local = instant.with_timezone(&tz);
    let date = local.date_naive();
    let aligned_date = match gran {
        Granularity::Day => date,
        Granularity::Week => date - Duration::days(date.weekday().num_days_from_monday() as i64),
        Granularity::Month => NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
            .expect("the first of a month is a valid date"),
        Granularity::Year => NaiveDate::from_ymd_opt(date.year(), 1, 1)
            .expect("the first of a month is a valid date"),
    };
    aligned_date
        .and_hms_opt(0, 0, 0)
        .expect("midnight is a valid time")
        .and_local_timezone(tz)
        .single()
        .expect("a fixed offset has no ambiguous local time")
        .with_timezone(&Utc)
}
