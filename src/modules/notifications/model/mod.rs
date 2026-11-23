//! Stored shapes of notifications and per-user read marks. Both are stored flat (no nested
//! objects), so the same columns serve SQLite, MongoDB and the sync wire unchanged.

mod notification_document;
mod notification_read_document;

pub use notification_document::AudienceKind;
pub use notification_document::NotificationDocument;
pub use notification_read_document::NotificationReadDocument;
