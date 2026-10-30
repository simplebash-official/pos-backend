use super::sync_cloud_store::MIN_DESKTOP_APP_VERSION;
use super::sync_cloud_store::parse_app_version;

#[test]
fn app_versions_parse_and_compare_numerically() {
    assert_eq!(parse_app_version("1.13.0"), Some((1, 13, 0)));
    assert_eq!(parse_app_version("1.13.2-dev"), Some((1, 13, 2)));
    assert!(parse_app_version("1.9.0").unwrap() < MIN_DESKTOP_APP_VERSION);
    assert!(parse_app_version("1.12.0").unwrap() < MIN_DESKTOP_APP_VERSION);
    assert!(parse_app_version("2.0.0").unwrap() > MIN_DESKTOP_APP_VERSION);
    for bad in ["", "1.13", "1.13.0.1", "v1.13.0", "one.two.three"] {
        assert_eq!(parse_app_version(bad), None, "{bad}");
    }
}
