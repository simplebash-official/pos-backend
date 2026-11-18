use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Longest title, in characters.
pub const MAX_TITLE_CHARS: usize = 120;
/// Longest message, in characters.
pub const MAX_BODY_CHARS: usize = 500;
/// Longest link button text, in characters.
pub const MAX_LINK_LABEL_CHARS: usize = 40;
/// Longest link path, in characters.
pub const MAX_LINK_PATH_CHARS: usize = 300;
/// Longest dedupe key, in characters.
pub const MAX_DEDUPE_KEY_CHARS: usize = 200;

/// Which part of the shop a notification is about; the history filters on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationCategory {
    Sales,
    Repairs,
    PrintJobs,
    Inventory,
    Expenses,
    Warranties,
    Cash,
    System,
    Announcements,
}

impl NotificationCategory {
    /// Every category, in the order the screens list them.
    pub const ALL: [NotificationCategory; 9] = [
        NotificationCategory::Sales,
        NotificationCategory::Repairs,
        NotificationCategory::PrintJobs,
        NotificationCategory::Inventory,
        NotificationCategory::Expenses,
        NotificationCategory::Warranties,
        NotificationCategory::Cash,
        NotificationCategory::System,
        NotificationCategory::Announcements,
    ];

    /// The stored and wire name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationCategory::Sales => "sales",
            NotificationCategory::Repairs => "repairs",
            NotificationCategory::PrintJobs => "print_jobs",
            NotificationCategory::Inventory => "inventory",
            NotificationCategory::Expenses => "expenses",
            NotificationCategory::Warranties => "warranties",
            NotificationCategory::Cash => "cash",
            NotificationCategory::System => "system",
            NotificationCategory::Announcements => "announcements",
        }
    }

    /// Reads a stored or wire name; `None` for anything else.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == value)
    }
}

/// How much a notification matters; drives its colour and whether it pops up on arrival.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationSeverity {
    Info,
    Success,
    Warning,
    Critical,
}

impl NotificationSeverity {
    /// Every severity, mildest first.
    pub const ALL: [NotificationSeverity; 4] = [
        NotificationSeverity::Info,
        NotificationSeverity::Success,
        NotificationSeverity::Warning,
        NotificationSeverity::Critical,
    ];

    /// The stored and wire name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationSeverity::Info => "info",
            NotificationSeverity::Success => "success",
            NotificationSeverity::Warning => "warning",
            NotificationSeverity::Critical => "critical",
        }
    }

    /// Reads a stored or wire name; `None` for anything else.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == value)
    }
}

/// Who a notification is for. Read state is kept per user either way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationAudience {
    /// Every user of the shop.
    Everyone,
    /// Only users whose role grants this permission (`module:action`).
    Permission { permission: String },
    /// One user, by their user id.
    User {
        #[serde(rename = "userKey")]
        user_key: String,
    },
}

/// Where a notification came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationSource {
    /// Raised by the POS itself (a sale, a repair, the app).
    Shop,
    /// Sent by a trusted internal service over the internal API.
    Service,
}

impl NotificationSource {
    /// The stored and wire name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationSource::Shop => "shop",
            NotificationSource::Service => "service",
        }
    }

    /// Reads a stored name; `None` for anything else.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "shop" => Some(NotificationSource::Shop),
            "service" => Some(NotificationSource::Service),
            _ => None,
        }
    }
}

/// A button on a notification that opens a screen of the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotificationLink {
    /// App route, always starting with `/` (e.g. `/repairs?job=rep_…`).
    pub path: String,
    /// Button text, e.g. "Open job".
    pub label: String,
}

/// One notification as a user sees it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub key: String,
    pub category: NotificationCategory,
    pub severity: NotificationSeverity,
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<NotificationLink>,
    pub audience: NotificationAudience,
    pub source: NotificationSource,
    /// Whether the caller has read it (worked out per user on every read).
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

/// What a producer hands the engine; the engine adds key, time and read state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewNotification {
    pub category: NotificationCategory,
    pub severity: NotificationSeverity,
    pub title: String,
    pub body: String,
    pub link: Option<NotificationLink>,
    pub audience: NotificationAudience,
    pub source: NotificationSource,
    /// Same value, same notification: a repeat (a retry, a second terminal, a replay) is absorbed.
    pub dedupe_key: Option<String>,
    /// The user who caused it, when a person did.
    pub raised_by_user_key: Option<String>,
}

impl NewNotification {
    /// Trims the text and checks every limit. The messages are for the person or service that
    /// sent it.
    pub fn validated(mut self) -> Result<Self, String> {
        self.title = required_text("title", &self.title, MAX_TITLE_CHARS)?;
        self.body = required_text("message", &self.body, MAX_BODY_CHARS)?;
        if let Some(link) = self.link.take() {
            let path = required_text("link path", &link.path, MAX_LINK_PATH_CHARS)?;
            // An app route only: never another site, never a protocol-relative URL.
            if !path.starts_with('/') || path.starts_with("//") {
                return Err("The link must be a page of the app, starting with '/'".to_string());
            }
            let label = required_text("link label", &link.label, MAX_LINK_LABEL_CHARS)?;
            self.link = Some(NotificationLink { path, label });
        }
        match &mut self.audience {
            NotificationAudience::Everyone => {}
            NotificationAudience::Permission { permission } => {
                *permission = required_text("permission", permission, 80)?;
            }
            NotificationAudience::User { user_key } => {
                *user_key = required_text("user", user_key, 80)?;
            }
        }
        if let Some(dedupe) = self.dedupe_key.take() {
            self.dedupe_key = Some(required_text("dedupe key", &dedupe, MAX_DEDUPE_KEY_CHARS)?);
        }
        Ok(self)
    }
}

fn required_text(field: &str, value: &str, max_chars: usize) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("The {field} cannot be empty"));
    }
    if trimmed.chars().count() > max_chars {
        return Err(format!("The {field} can be at most {max_chars} characters"));
    }
    Ok(trimmed.to_string())
}

/// The key of a notification raised under `dedupe_key`: the same on every replica, so the same
/// alert raised on two terminals (or replayed by a service) converges to one row.
#[must_use]
pub fn notification_key_for(dedupe_key: &str) -> String {
    format!(
        "{}_{}",
        crate::core::constants::prefixes::NOTIFICATION,
        short_hash(dedupe_key)
    )
}

/// The key of `user_key`'s read mark on `notification_key`: one per user and notification on
/// every replica.
#[must_use]
pub fn read_key_for(user_key: &str, notification_key: &str) -> String {
    format!(
        "{}_{}",
        crate::core::constants::prefixes::NOTIFICATION_READ,
        short_hash(&format!("{user_key}|{notification_key}"))
    )
}

fn short_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(value.as_bytes());
    // 12 bytes = 96 bits: collisions are not a practical concern within one shop.
    digest.iter().take(12).map(|b| format!("{b:02x}")).collect()
}
