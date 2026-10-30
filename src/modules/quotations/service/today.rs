use chrono::Utc;

/// `YYYY-MM-DD` for today, UTC — the same day boundary every other module uses.
pub(super) fn today() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}
