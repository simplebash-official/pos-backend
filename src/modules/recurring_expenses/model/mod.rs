// Mongo document shape for regular bills, kept apart from the API-facing
// `domain::recurring_expenses` so BSON types never leak into payloads.

mod recurring_expense_document;

pub use recurring_expense_document::RecurringExpenseDocument;
