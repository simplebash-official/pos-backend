//! Mongo document shape for an account's opening balance, kept apart from the API-facing
//! `domain::accounts` so BSON types never leak into payloads.

mod account_opening_document;

pub use account_opening_document::AccountOpeningDocument;
