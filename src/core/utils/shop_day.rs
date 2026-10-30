use super::shop_tz_offset_secs::SHOP_TZ_OFFSET_SECS;

use chrono::{DateTime, Utc};

/// The shop-local calendar day that `at` falls on.
#[must_use]
pub fn shop_day(at: DateTime<Utc>) -> chrono::NaiveDate {
    at.with_timezone(
        &chrono::FixedOffset::east_opt(SHOP_TZ_OFFSET_SECS).expect("valid fixed offset"),
    )
    .date_naive()
}
