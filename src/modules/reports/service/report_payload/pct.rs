/// bps -> `"12.3%"`.
pub(super) fn pct(bps: i64) -> String {
    format!("{}.{}%", bps / 100, (bps.abs() % 100) / 10)
}
