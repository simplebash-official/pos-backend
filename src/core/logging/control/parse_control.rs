use super::control_line::ControlLine;

/// Parses one control line; `None` for anything that isn't a `log_mode` command.
#[must_use]
pub fn parse_control(line: &str) -> Option<ControlLine> {
    let parsed: ControlLine = serde_json::from_str(line.trim()).ok()?;
    (parsed.cmd == "log_mode").then_some(parsed)
}
