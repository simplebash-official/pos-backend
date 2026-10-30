use chrono::Utc;

pub(super) fn today() -> String {
    Utc::now().date_naive().format("%Y-%m-%d").to_string()
}
