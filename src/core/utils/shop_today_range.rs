use super::shop_day::shop_day;
use super::shop_tz_offset_secs::SHOP_TZ_OFFSET_SECS;

use chrono::{DateTime, Duration, Utc};

/// Returns the `[start, end)` bounds, as UTC instants, of the shop-local "today" (midnight to
/// midnight in `SHOP_TZ_OFFSET_SECS`), for the "today's X" stats and list filters.
#[must_use]
pub fn shop_today_range() -> (DateTime<Utc>, DateTime<Utc>) {
    let start = shop_day(Utc::now())
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is always a valid time")
        .and_utc()
        - Duration::seconds(i64::from(SHOP_TZ_OFFSET_SECS));
    (start, start + Duration::days(1))
}
