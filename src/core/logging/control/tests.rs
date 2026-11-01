use super::super::SqlLogging;
use super::logging_control::parse_control;

#[test]
fn parses_log_mode_lines_and_ignores_others() {
    let parsed =
        parse_control(r#"{"cmd":"log_mode","enabled":false,"http_bodies":false,"sql":"off"}"#)
            .expect("log_mode line");
    assert!(!parsed.enabled);
    assert_eq!(parsed.sql.as_deref(), Some("off"));

    // Unknown command, malformed JSON and plain text are all ignored.
    assert!(parse_control(r#"{"cmd":"something_else"}"#).is_none());
    assert!(parse_control("not json").is_none());
    assert!(parse_control("").is_none());

    // A line that omits a switch is ignored, never completed with a guess.
    assert!(parse_control(r#"{"cmd":"log_mode"}"#).is_none());
    assert!(parse_control(r#"{"cmd":"log_mode","enabled":true}"#).is_none());
}

#[test]
fn sql_codes_round_trip() {
    for mode in [SqlLogging::All, SqlLogging::Slow, SqlLogging::Off] {
        assert_eq!(SqlLogging::from_code(mode.code()), mode);
    }
    assert_eq!(SqlLogging::parse("ALL"), SqlLogging::All);
    assert_eq!(SqlLogging::parse("nonsense"), SqlLogging::Slow);
}
