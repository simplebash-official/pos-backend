/// The shop's local time zone. Sri Lanka is permanently UTC+05:30 (no DST since 2006), so a fixed
/// offset is exact. Everything is stored in UTC; this only decides where a day starts and ends.
pub const SHOP_TZ_OFFSET_SECS: i32 = 5 * 3600 + 30 * 60;
