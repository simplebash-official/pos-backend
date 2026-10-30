/// 1% == 100 bps. Returns `num/den` as basis points, or 0 when `den` is 0.
pub(super) fn bps(num: i64, den: i64) -> i64 {
    if den == 0 {
        0
    } else {
        ((num as i128 * 10_000) / den as i128) as i64
    }
}
