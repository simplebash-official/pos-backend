use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A supplier-product link as returned to API clients. `supplierKey`/
/// `productKey` are the FKs this link joins — always a document `key`, per
/// this codebase's cross-reference convention, never a raw `ObjectId`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierProductLink {
    /// Unique business key identifying this link record (e.g. spl_...).
    pub key: String,
    /// Foreign key referencing the supplier.
    pub supplier_key: String,
    /// Foreign key referencing the product.
    pub product_key: String,
    /// Specific purchase cost price from this supplier in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_price_cents: Option<i64>,
    /// Optional supplier remarks or SKU reference notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Date when product was linked to supplier.
    pub added_at: DateTime<Utc>,
    /// Timestamp when link was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when link was last modified.
    pub updated_at: DateTime<Utc>,
    /// Optimistic concurrency control version number.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}
