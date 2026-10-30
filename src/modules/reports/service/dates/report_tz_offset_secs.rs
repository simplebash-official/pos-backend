/// Shop-local timezone for reporting. Sri Lanka is permanently UTC+05:30 — no
/// DST since 2006 — so a fixed offset is exact and needs no `chrono-tz`
/// dependency. The new analytics pipelines bucket and label by this local
/// time (via `parse_date_range_tz` for the `$match` bounds and the
/// `REPORT_TZ_MONGO` string passed to `$dateTrunc` / `$hour` / `$isoDayOfWeek`)
/// so "Tuesday", "August", and "9am" mean what the shop owner expects; a
/// bare-UTC bucket pushes the local 00:00–05:29 slice into the previous day.
/// The older endpoints (`dashboard`, `daily-sales`, `monthly-profit`, …) stay
/// on the UTC `parse_date_range` below — they are almost always "today" /
/// "this_month" where the boundary barely moves.
pub(crate) const REPORT_TZ_OFFSET_SECS: i32 = crate::core::utils::SHOP_TZ_OFFSET_SECS;
