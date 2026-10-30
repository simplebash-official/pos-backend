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
