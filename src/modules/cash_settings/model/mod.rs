// Mongo document shape for the drawer rules, kept apart from the API-facing
// `domain::cash_settings` so BSON types never leak into payloads.

mod cash_settings_document;

pub use cash_settings_document::CashSettingsDocument;
