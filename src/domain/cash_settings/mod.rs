//! Pure business types for the shop's drawer rules. No I/O, no framework types beyond serde/utoipa
//! derives. The values live in one stored row; the shipped starting values (`STARTING_*`) only seed
//! that row and are never read as a fallback.

// The file is named after its type, which shares the module name.
#[allow(clippy::module_inception)]
mod cash_settings;
mod domain_cash_settings;

pub use cash_settings::CashSettings;
pub use domain_cash_settings::MAX_QUICK_AMOUNTS;
pub use domain_cash_settings::MAX_RULE_CENTS;
pub use domain_cash_settings::STARTING_QUICK_AMOUNTS_CENTS;
pub use domain_cash_settings::STARTING_UNDO_WINDOW_SECONDS;
pub use domain_cash_settings::UNDO_WINDOW_RANGE;
pub use domain_cash_settings::UpdateCashSettingsRequest;
