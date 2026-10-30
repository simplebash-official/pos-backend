use super::report_tz_offset_secs::REPORT_TZ_OFFSET_SECS;

use chrono::FixedOffset;

pub(crate) fn report_tz() -> FixedOffset {
    FixedOffset::east_opt(REPORT_TZ_OFFSET_SECS).expect("valid fixed offset")
}
