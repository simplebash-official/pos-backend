//! Mongo document shape for the income ledger, kept apart from the API-facing
//! `domain::incomes` so BSON types never leak into payloads.

mod income_document;

pub use income_document::IncomeDocument;
