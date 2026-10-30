// Mongo document shapes for balance openings and advance recoveries, kept apart from the
// API-facing `domain::balances` so BSON types never leak into payloads.

mod advance_recovery_document;
mod balance_opening_document;
mod default_version;

pub use advance_recovery_document::AdvanceRecoveryDocument;
pub use balance_opening_document::BalanceOpeningDocument;
