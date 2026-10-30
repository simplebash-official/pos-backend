use super::super::settings;
use super::apply_control::apply_control;
use super::parse_control::parse_control;

use tokio::io::{AsyncBufReadExt, BufReader};

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
