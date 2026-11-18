use crate::core::error::{AppError, AppResult};
use crate::domain::notifications::{
    Notification, NotificationAudience, NotificationCategory, NotificationLink,
    NotificationSeverity, NotificationSource,
};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// How the audience is stored: its kind, with the permission or user in `audience_value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudienceKind {
    Everyone,
    Permission,
    User,
}

impl AudienceKind {
    /// The stored name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AudienceKind::Everyone => "everyone",
            AudienceKind::Permission => "permission",
            AudienceKind::User => "user",
        }
    }

    /// Reads a stored name; `None` for anything else.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "everyone" => Some(AudienceKind::Everyone),
            "permission" => Some(AudienceKind::Permission),
            "user" => Some(AudienceKind::User),
            _ => None,
        }
    }
}

/// How a notification is stored. Never edited after it is written; the retention worker
/// removes it after `NOTIFICATION_RETENTION_DAYS`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub category: NotificationCategory,
    pub severity: NotificationSeverity,
    pub title: String,
    pub body: String,
    /// Set together with `link_label`, or neither.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_label: Option<String>,
    pub audience_type: AudienceKind,
    /// The permission or user key; absent for `everyone`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience_value: Option<String>,
    pub source: NotificationSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raised_by_user_key: Option<String>,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl NotificationDocument {
    /// Splits an audience into its stored kind and value.
    #[must_use]
    pub fn stored_audience(audience: &NotificationAudience) -> (AudienceKind, Option<String>) {
        match audience {
            NotificationAudience::Everyone => (AudienceKind::Everyone, None),
            NotificationAudience::Permission { permission } => {
                (AudienceKind::Permission, Some(permission.clone()))
            }
            NotificationAudience::User { user_key } => (AudienceKind::User, Some(user_key.clone())),
        }
    }

    /// The API shape, with the caller's read state.
    pub fn into_notification(self, read: bool) -> AppResult<Notification> {
        let corrupt = |what: &str| {
            AppError::internal(format!("Notification {} has {what} in database", self.key))
        };
        let audience = match (self.audience_type, &self.audience_value) {
            (AudienceKind::Everyone, None) => NotificationAudience::Everyone,
            (AudienceKind::Permission, Some(permission)) => NotificationAudience::Permission {
                permission: permission.clone(),
            },
            (AudienceKind::User, Some(user_key)) => NotificationAudience::User {
                user_key: user_key.clone(),
            },
            _ => return Err(corrupt("an audience without its value")),
        };
        let link = match (&self.link_path, &self.link_label) {
            (Some(path), Some(label)) => Some(NotificationLink {
                path: path.clone(),
                label: label.clone(),
            }),
            (None, None) => None,
            _ => return Err(corrupt("half a link")),
        };
        Ok(Notification {
            key: self.key,
            category: self.category,
            severity: self.severity,
            title: self.title,
            body: self.body,
            link,
            audience,
            source: self.source,
            read,
            created_at: self.created_at.to_chrono(),
        })
    }
}
