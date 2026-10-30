use super::is_safe_path_segment::is_safe_path_segment;

#[test]
fn only_generated_key_shapes_are_safe_path_segments() {
    for ok in [
        "inv_V1StGXR8_Z5jdHi6B-myT",
        "thermal-receipt-80mm",
        "a4-invoice",
    ] {
        assert!(is_safe_path_segment(ok), "{ok}");
    }
    for bad in [
        "",
        "..",
        "../../etc",
        "/tmp/x",
        "inv_1/..",
        "C:\\x",
        "a b",
        "inv\u{0}",
    ] {
        assert!(!is_safe_path_segment(bad), "{bad:?}");
    }
    assert!(!is_safe_path_segment(&"a".repeat(129)));
}
