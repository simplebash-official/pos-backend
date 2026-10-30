/// `a / b` rounded half up (towards +infinity), the same as JavaScript's `Math.round`.
pub(super) fn round_half_up(numerator: i128, denominator: i128) -> i128 {
    (2 * numerator + denominator).div_euclid(2 * denominator)
}
