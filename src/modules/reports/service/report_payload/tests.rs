use super::service_report_payload::money;
use super::service_report_payload::pct;

#[test]
fn money_formats_lkr_with_thousands_and_paisa() {
    assert_eq!(money(0), "Rs. 0.00");
    assert_eq!(money(100), "Rs. 1.00");
    assert_eq!(money(12_345), "Rs. 123.45");
    assert_eq!(money(1_234_567), "Rs. 12,345.67");
    assert_eq!(money(-500_000), "-Rs. 5,000.00");
}

#[test]
fn pct_formats_bps_as_one_decimal_percent() {
    assert_eq!(pct(1_234), "12.3%");
    assert_eq!(pct(0), "0.0%");
    assert_eq!(pct(10_000), "100.0%");
}
