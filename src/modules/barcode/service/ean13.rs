// Pure EAN-13 encoding — no I/O, no DB. Uses the GS1-reserved `20`-`29`
// prefix range (standard practice for in-store/internal-use barcodes, the
// same mechanism grocery stores use for scale/deli labels) so generated
// codes never collide with a real published GTIN.

use crate::core::error::{AppError, AppResult};

/// Maps a barcode namespace (a caller-owned string, e.g. `"product"`) to
/// its fixed 2-digit reserved prefix. An explicit per-namespace registry
/// rather than a hash — so two namespaces can never be assigned the same
/// prefix, which combined with each namespace's counter independently
/// starting at 1 would otherwise let two different namespaces mint the same
/// barcode. Add a new arm here when a new label-worthy entity needs one.
fn prefix_for_namespace(namespace: &str) -> AppResult<&'static str> {
    match namespace {
        "product" => Ok("20"),
        other => Err(AppError::internal(format!(
            "no reserved EAN-13 prefix registered for barcode namespace '{other}'"
        ))),
    }
}

/// Standard EAN-13 check digit algorithm: digits at odd positions
/// (1-indexed, left to right) are weighted 1, even positions weighted 3;
/// the check digit is `(10 - (sum % 10)) % 10`.
fn checksum_digit(twelve_digits: &str) -> u8 {
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

/// Encodes `sequence` under `namespace`'s reserved prefix into a raw
/// 13-digit numeric string (no dashes/spacing — display formatting, if
/// ever needed, happens at render/print time, never here). Errors if
/// `sequence` doesn't fit in the 10-digit sequence field, i.e. the
/// namespace's counter space is exhausted — expected to never happen in
/// practice.
pub(crate) fn encode(namespace: &str, sequence: i64) -> AppResult<String> {
    let prefix = prefix_for_namespace(namespace)?;

    if !(0..=9_999_999_999).contains(&sequence) {
        return Err(AppError::internal(format!(
            "barcode sequence {sequence} out of range for namespace '{namespace}'"
        )));
    }

    let twelve = format!("{prefix}{sequence:010}");
    let check = checksum_digit(&twelve);
    Ok(format!("{twelve}{check}"))
}

#[cfg(test)]
mod tests {
    use super::{checksum_digit, encode};

    #[test]
    fn checksum_digit_matches_known_example() {
        // 4006381333931 is a commonly cited EAN-13 reference vector.
        assert_eq!(checksum_digit("400638133393"), 1);
    }

    #[test]
    fn encode_produces_thirteen_digit_numeric_string() {
        let barcode = encode("product", 1).unwrap();
        assert_eq!(barcode, "2000000000015");
        assert_eq!(barcode.len(), 13);
        assert!(barcode.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn encode_is_deterministic() {
        assert_eq!(
            encode("product", 42).unwrap(),
            encode("product", 42).unwrap()
        );
    }

    #[test]
    fn encode_varies_with_sequence() {
        assert_ne!(encode("product", 1).unwrap(), encode("product", 2).unwrap());
    }

    #[test]
    fn encode_rejects_unknown_namespace() {
        assert!(encode("bogus", 1).is_err());
    }

    #[test]
    fn encode_rejects_out_of_range_sequence() {
        assert!(encode("product", 10_000_000_000).is_err());
        assert!(encode("product", -1).is_err());
    }
}
