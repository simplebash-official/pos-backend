// Mongo document shape for drawer cash categories, kept apart from the
// API-facing `domain::cash_categories` so BSON types never leak into payloads.

mod cash_category_document;

pub use cash_category_document::CashCategoryDocument;
