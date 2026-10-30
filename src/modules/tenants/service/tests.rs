use super::normalize_shop_code::normalize_shop_code;

#[test]
fn shop_codes_are_lowercased_and_validated() {
    assert_eq!(
        normalize_shop_code("  Acme-Repairs ").unwrap(),
        "acme-repairs"
    );
    for bad in [
        "ab",
        "-abc",
        "abc-",
        "has space",
        "under_score",
        "",
        &"x".repeat(33),
    ] {
        assert!(normalize_shop_code(bad).is_err(), "{bad:?}");
    }
}
