/// Standard EAN-13 check digit algorithm: digits at odd positions
/// (1-indexed, left to right) are weighted 1, even positions weighted 3;
/// the check digit is `(10 - (sum % 10)) % 10`.
pub(super) fn checksum_digit(twelve_digits: &str) -> u8 {
    let sum: u32 = twelve_digits
        .chars()
        .enumerate()
        .map(|(i, c)| {
            let digit = c
                .to_digit(10)
                .expect("twelve_digits is always ASCII digits");
            let weight = if (i + 1) % 2 == 1 { 1 } else { 3 };
            digit * weight
        })
        .sum();

    ((10 - (sum % 10)) % 10) as u8
}
