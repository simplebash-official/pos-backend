//! Pure business types for shop notifications: what a notification is, who it is for, the
//! history filter and the request/response shapes. No I/O. Toasts (brief pop-ups) never reach
//! here; only alerts worth keeping in the history do.

// The file is named after its type, which shares the module name.
#[allow(clippy::module_inception)]
mod notification;
mod notification_query;
#[cfg(test)]
mod tests;

pub use notification::MAX_BODY_CHARS;
pub use notification::MAX_TITLE_CHARS;
pub use notification::NewNotification;
pub use notification::Notification;
pub use notification::NotificationAudience;
pub use notification::NotificationCategory;
pub use notification::NotificationLink;
pub use notification::NotificationSeverity;
pub use notification::NotificationSource;
pub use notification::notification_key_for;
pub use notification::read_key_for;
pub use notification_query::AppAudience;
pub use notification_query::CategoryUnread;
pub use notification_query::CreateNotificationRequest;
pub use notification_query::HistoryCursor;
pub use notification_query::InternalNotificationRequest;
pub use notification_query::MAX_MARK_KEYS;
pub use notification_query::MAX_PAGE_LIMIT;
pub use notification_query::MarkAllReadRequest;
pub use notification_query::MarkReadRequest;
pub use notification_query::MarkReadResponse;
pub use notification_query::NotificationFilter;
pub use notification_query::NotificationListQuery;
pub use notification_query::NotificationPage;
pub use notification_query::NotificationSummary;
pub use notification_query::ReadState;
pub use notification_query::SUMMARY_LATEST;
