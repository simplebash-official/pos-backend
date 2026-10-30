//! Business rules for the drawer rules: seeding the row with the shipped starting values, validating
//! edits, and reading the rules for the cash movement code. A missing row is an error, never a default.

mod cash_settings_service;
#[cfg(test)]
mod tests;

pub(crate) use cash_settings_service::approval_threshold_cents;
pub use cash_settings_service::ensure_settings;
pub use cash_settings_service::get_settings;
pub(crate) use cash_settings_service::undo_window_seconds;
pub use cash_settings_service::update_settings;
#[cfg(test)]
use cash_settings_service::validate;
