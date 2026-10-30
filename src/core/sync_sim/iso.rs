use chrono::{SecondsFormat, TimeZone, Utc};

pub(super) fn iso(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .expect("valid millis")
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}
