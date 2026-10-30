use chrono::Utc;

pub(super) fn now_iso() -> String {
    Utc::now().to_rfc3339()
}
