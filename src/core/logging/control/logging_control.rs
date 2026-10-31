use super::super::{SqlLogging, settings};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};

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

#[derive(Debug, Deserialize, PartialEq, Eq)]
/// One line written to stdin by the desktop shell to change the logging mode.
pub struct ControlLine {
    pub cmd: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub http_bodies: bool,
    pub sql: Option<String>,
}

fn default_true() -> bool {
    true
}

/// The logging switches in force after a control line was applied.
#[derive(Debug, Clone, Copy)]
pub struct LogMode {
    pub enabled: bool,
    pub http_bodies: bool,
    pub sql: SqlLogging,
}

/// Parses one control line; `None` for anything that isn't a `log_mode` command.
#[must_use]
pub fn parse_control(line: &str) -> Option<ControlLine> {
    let parsed: ControlLine = serde_json::from_str(line.trim()).ok()?;
    (parsed.cmd == "log_mode").then_some(parsed)
}

/// Reads control lines from stdin for the life of the process (desktop only).
pub fn spawn_stdin_control() {
    if !settings().json {
        return;
    }
    tokio::spawn(async move {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            match parse_control(&line) {
                Some(control) => {
                    let mode = apply_control(&control);
                    // Logged after applying, so an "off" switch is the last line.
                    tracing::info!(
                        category = "system",
                        event = "log_mode",
                        enabled = mode.enabled,
                        http_bodies = mode.http_bodies,
                        sql = ?mode.sql,
                        "logging mode changed by the desktop shell"
                    );
                }
                None => tracing::debug!(
                    category = "system",
                    event = "control.ignored",
                    "ignored a control line"
                ),
            }
        }
    });
}
