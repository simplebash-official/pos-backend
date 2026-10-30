use super::log_settings::LogSettings;
use std::sync::OnceLock;

pub fn settings() -> &'static LogSettings {
    static SETTINGS: OnceLock<LogSettings> = OnceLock::new();
    SETTINGS.get_or_init(LogSettings::from_env)
}
