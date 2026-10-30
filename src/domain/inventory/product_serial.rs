use super::default_version::default_version;
use super::serial_status::SerialStatus;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One physical serialized unit of a serialized product.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductSerial {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Unique business key of this serial record (e.g. psn_...).
    pub key: String,
    /// Key of the product this unit belongs to.
    pub product_key: String,
    /// The manufacturer/shop serial number, unique across all products.
    pub serial_number: String,
    /// The stock batch this unit was received in — it sells at that batch's
    /// price and its cost is that batch's cost.
    pub batch_key: String,
    /// Current lifecycle status of this unit.
    pub status: SerialStatus,
    /// Key of the invoice this unit was sold on, once `Sold`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_key: Option<String>,
    /// When this unit was sold, once `Sold`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sold_at: Option<DateTime<Utc>>,
    /// Warranty length snapshotted from the product at the moment of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_months: Option<i64>,
    /// Computed `sold_at + warranty_months`, used to decide `within_warranty`
    /// on a later credit-note return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_expires_at: Option<DateTime<Utc>>,
    /// Key of the credit note that most recently returned this unit, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_note_key: Option<String>,
    /// Timestamp when this serial record was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when this serial record was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version.
    #[serde(default = "default_version")]
    pub version: i64,
}
