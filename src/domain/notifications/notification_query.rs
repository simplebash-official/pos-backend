use super::notification::{
    Notification, NotificationAudience, NotificationCategory, NotificationLink,
    NotificationSeverity,
};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Most notifications one history page returns.
pub const MAX_PAGE_LIMIT: u32 = 50;
/// How many recent notifications the bell's summary carries.
pub const SUMMARY_LATEST: i64 = 30;
/// Most keys one `POST /notifications/read` marks.
pub const MAX_MARK_KEYS: usize = 100;
/// Longest search text, in characters.
pub const MAX_SEARCH_CHARS: usize = 100;

/// Which notifications to list by the caller's read state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadState {
    All,
    Unread,
    Read,
}

/// Query for `GET /notifications`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct NotificationListQuery {
    /// Read state to list.
    pub state: ReadState,
    /// Page size, 1 to 50.
    pub limit: u32,
    /// Comma-separated categories (`sales,repairs`); absent lists every category.
    pub categories: Option<String>,
    /// Comma-separated severities (`warning,critical`); absent lists every severity.
    pub severities: Option<String>,
    /// Only notifications raised at or after this moment (RFC 3339).
    pub from: Option<DateTime<Utc>>,
    /// Only notifications raised before this moment (RFC 3339).
    pub to: Option<DateTime<Utc>>,
    /// Text to look for in the title or message.
    pub q: Option<String>,
    /// `nextCursor` of the previous page.
    pub cursor: Option<String>,
}

/// The filter shared by the history list and "mark these as read", already checked.
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationFilter {
    pub categories: Vec<NotificationCategory>,
    pub severities: Vec<NotificationSeverity>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// Trimmed, non-empty search text.
    pub search: Option<String>,
}

impl NotificationFilter {
    /// Parses the comma lists and search text sent by a client.
    pub fn parse(
        categories: Option<&str>,
        severities: Option<&str>,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        q: Option<&str>,
    ) -> Result<Self, String> {
        let categories = parse_list(categories, "category", NotificationCategory::parse)?;
        let severities = parse_list(severities, "severity", NotificationSeverity::parse)?;
        if let (Some(from), Some(to)) = (from, to)
            && from >= to
        {
            return Err("The start of the date range must be before its end".to_string());
        }
        let search = match q.map(str::trim) {
            None | Some("") => None,
            Some(text) if text.chars().count() > MAX_SEARCH_CHARS => {
                return Err(format!(
                    "Search text can be at most {MAX_SEARCH_CHARS} characters"
                ));
            }
            Some(text) => Some(text.to_string()),
        };
        Ok(NotificationFilter {
            categories,
            severities,
            from,
            to,
            search,
        })
    }
}

fn parse_list<T>(
    value: Option<&str>,
    what: &str,
    parse: fn(&str) -> Option<T>,
) -> Result<Vec<T>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for part in value.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        out.push(parse(part).ok_or_else(|| format!("Unknown {what} '{part}'"))?);
    }
    Ok(out)
}

/// A position in the newest-first history: the last row a page returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryCursor {
    pub created_at: DateTime<Utc>,
    pub key: String,
}

impl HistoryCursor {
    /// The opaque `nextCursor` text (`<epoch millis>:<key>`).
    #[must_use]
    pub fn encode(&self) -> String {
        format!("{}:{}", self.created_at.timestamp_millis(), self.key)
    }

    /// Reads a `nextCursor` sent back by a client.
    pub fn decode(value: &str) -> Result<Self, String> {
        let invalid = || "This page link is no longer valid; reload the list".to_string();
        let (millis, key) = value.split_once(':').ok_or_else(invalid)?;
        let millis: i64 = millis.parse().map_err(|_| invalid())?;
        let created_at = Utc
            .timestamp_millis_opt(millis)
            .single()
            .ok_or_else(invalid)?;
        if key.is_empty() {
            return Err(invalid());
        }
        Ok(HistoryCursor {
            created_at,
            key: key.to_string(),
        })
    }
}

/// One page of the history, newest first.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPage {
    pub items: Vec<Notification>,
    /// Send back as `cursor` for the next page; absent on the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Unread count of one category.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryUnread {
    pub category: NotificationCategory,
    pub unread: u64,
}

/// What the header bell shows: counts plus the newest notifications.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSummary {
    pub unread_total: u64,
    /// Only categories with at least one unread notification, in `NotificationCategory::ALL` order.
    pub unread_by_category: Vec<CategoryUnread>,
    /// The newest 30 visible notifications, read or not.
    pub latest: Vec<Notification>,
}

/// Body for `POST /notifications/read`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarkReadRequest {
    /// 1 to 100 notification keys.
    pub keys: Vec<String>,
}

/// Body for `POST /notifications/read-all`: marks every unread notification matching the filter.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarkAllReadRequest {
    pub categories: Option<String>,
    pub severities: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub q: Option<String>,
}

/// How many notifications a mark-as-read call changed.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarkReadResponse {
    pub marked: u64,
}

/// Who an alert raised by the app is for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppAudience {
    /// Every user of the shop.
    Everyone,
    /// The users holding a permission the caller holds too.
    Permission { permission: String },
    /// Only the caller (a personal reminder).
    Me,
}

/// Body for `POST /notifications`: an alert raised by the app.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateNotificationRequest {
    pub category: NotificationCategory,
    pub severity: NotificationSeverity,
    pub title: String,
    pub body: String,
    pub link: Option<NotificationLink>,
    pub audience: AppAudience,
    /// Same value, same notification (once per shop, however many tabs raise it).
    pub dedupe_key: Option<String>,
}

/// Body for `POST /api/internal/tenants/{tenant_id}/notifications`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InternalNotificationRequest {
    pub category: NotificationCategory,
    pub severity: NotificationSeverity,
    pub title: String,
    pub body: String,
    pub link: Option<NotificationLink>,
    /// Everyone, or the users holding one permission.
    pub audience: NotificationAudience,
    /// Required, so a retried call cannot notify twice.
    pub dedupe_key: String,
}
