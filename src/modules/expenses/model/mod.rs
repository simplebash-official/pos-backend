//! Mongo document shapes for the expense ledger, kept apart from the API-facing
//! `domain::expenses` so BSON types never leak into payloads.

mod expenses_model;

pub use expenses_model::ExpenseDocument;
pub use expenses_model::ExpensePaymentDocument;
