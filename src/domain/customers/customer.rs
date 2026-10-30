use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A customer as returned to API clients. `id` (the Mongo `ObjectId` as a
/// hex string) is the stable route/lookup key; `key` is the human-shareable
/// prefixed id (see `core::id::generate_id`), used by other modules (`billing`,
/// `repairs`, etc.) to reference this customer without holding its `ObjectId`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Customer {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key (e.g. cus_...).
    pub key: String,
    /// Full customer name or business name.
    pub name: String,
    /// Point of contact person for organizations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_person: Option<String>,
    /// Primary contact phone number.
    pub primary_phone: String,
    /// Secondary contact phone number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_phone: Option<String>,
    /// Email address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Physical mailing or street address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// List of assigned customer category tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Freeform internal customer notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Outstanding debt/credit balance in cents.
    pub outstanding_balance_cents: i64,
    /// Total money spent across all purchases in cents.
    pub total_purchases_cents: i64,
    /// Timestamp when created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when last modified.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp of deletion if customer is soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier of client that last modified this customer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}
