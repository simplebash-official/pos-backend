use super::super::{SqlLogging, settings};
use super::control_line::ControlLine;
use super::log_mode::LogMode;

/// Applies a parsed line and returns what is now in force.
pub fn apply_control(line: &ControlLine) -> LogMode {
    let requested = line
        .sql
        .as_deref()
        .map_or(SqlLogging::Slow, SqlLogging::parse);
    // sqlx only emits statements if it was wired for them at connect time.
    let sql = match settings().sql_at_startup {
        SqlLogging::Off => SqlLogging::Off,
        _ => requested,
    };
    settings().apply(line.enabled, line.http_bodies, sql);
    LogMode {
        enabled: line.enabled,
        http_bodies: line.http_bodies,
        sql,
    }
}
