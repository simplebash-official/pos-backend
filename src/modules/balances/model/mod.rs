//! Mongo document shapes for balance openings and advance recoveries, kept apart from the
//! API-facing `domain::balances` so BSON types never leak into payloads.

mod balances_model;

pub use balances_model::AdvanceRecoveryDocument;
pub use balances_model::BalanceOpeningDocument;
