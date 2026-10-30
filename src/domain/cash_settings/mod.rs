// Pure business types for the shop's drawer rules. No I/O, no framework types beyond serde/utoipa
// derives. The values live in one stored row; the shipped starting values (`STARTING_*`) only seed
// that row and are never read as a fallback.

// The file is named after its type, which shares the module name.
#[allow(clippy::module_inception)]
mod cash_settings;
mod max_quick_amounts;
mod max_rule_cents;
mod starting_quick_amounts_cents;
mod starting_undo_window_seconds;
mod undo_window_range;
mod update_cash_settings_request;

pub use cash_settings::CashSettings;
pub use max_quick_amounts::MAX_QUICK_AMOUNTS;
pub use max_rule_cents::MAX_RULE_CENTS;
pub use starting_quick_amounts_cents::STARTING_QUICK_AMOUNTS_CENTS;
pub use starting_undo_window_seconds::STARTING_UNDO_WINDOW_SECONDS;
pub use undo_window_range::UNDO_WINDOW_RANGE;
pub use update_cash_settings_request::UpdateCashSettingsRequest;
