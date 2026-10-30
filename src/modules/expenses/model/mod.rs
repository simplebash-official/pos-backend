// Mongo document shapes for the expense ledger, kept apart from the API-facing
// `domain::expenses` so BSON types never leak into payloads.

mod default_version;
mod expense_document;
mod expense_payment_document;

pub use expense_document::ExpenseDocument;
pub use expense_payment_document::ExpensePaymentDocument;
