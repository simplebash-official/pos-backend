// Mongo document shape for quotations. Kept separate from
// `domain::quotations` (the API-facing types) so BSON concerns like
// `ObjectId` never leak into request/response payloads. Synced between devices
// and the cloud (see `modules::sync::resources`), hence `version`, `deleted_at`
// and `updated_by_device`.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::quotations::{Quotation, QuotationItem, STATUS_ACTIVE, SyncQuotation},
};

/// Mongo document shape representing a quotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotationDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key (`quo_...`).
    #[serde(default)]
    pub key: String,
    /// Sequential number (e.g. `QUO-0001`).
    pub quotation_number: String,
    /// `active` or `cancelled`; expiry is derived from `valid_until`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Customer name frozen at creation.
    pub customer_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer_address: Option<String>,
    pub items: Vec<QuotationItem>,
    pub subtotal_cents: i64,
    pub discount_type: String,
    pub discount_value: f64,
    pub discount_cents: i64,
    pub total_cents: i64,
    /// Last day the offer holds (`YYYY-MM-DD`). A string so Mongo and SQLite
    /// compare it identically.
    pub valid_until: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared_by: Option<String>,
    /// The shop profile as it was when the quotation was made, so a reprint
    /// after the shop changes its address or logo still matches the original.
    pub shop_profile_snapshot: serde_json::Value,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Sync version: 1 on create, +1 on every change (only `cancel` changes a
    /// quotation). `#[serde(default)]` keeps documents stored before sync loadable.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Always `None`: quotations are cancelled, never deleted. The column exists
    /// because the sync apply path writes it for every resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Device (or `web:<id>` / `cloud`) that last wrote this quotation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

impl QuotationDocument {
    /// The sync wire shape: every stored field, including the shop-profile
    /// snapshot that the REST `Quotation` leaves out.
    pub fn into_sync_quotation(self) -> SyncQuotation {
        let key = if self.key.is_empty() {
            generate_id(prefixes::QUOTATION)
        } else {
            self.key
        };
        SyncQuotation {
            id: self
                .id
                .expect("persisted quotation document must have an id")
                .to_hex(),
            key,
            quotation_number: self.quotation_number,
            status: self.status,
            customer_key: self.customer_key,
            customer_name: self.customer_name,
            customer_phone: self.customer_phone,
            customer_address: self.customer_address,
            items: self.items,
            subtotal_cents: self.subtotal_cents,
            discount_type: self.discount_type,
            discount_value: self.discount_value,
            discount_cents: self.discount_cents,
            total_cents: self.total_cents,
            valid_until: self.valid_until,
            notes: self.notes,
            terms: self.terms,
            prepared_by: self.prepared_by,
            shop_profile_snapshot: self.shop_profile_snapshot,
            version: self.version,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }

    /// `today` is `YYYY-MM-DD`; passed in so expiry is decided once per request.
    pub fn into_quotation(self, today: &str) -> Quotation {
        let key = if self.key.is_empty() {
            generate_id(prefixes::QUOTATION)
        } else {
            self.key
        };
        let is_expired = self.status == STATUS_ACTIVE && self.valid_until.as_str() < today;
        Quotation {
            id: self
                .id
                .expect("persisted quotation document must have an id")
                .to_hex(),
            key,
            quotation_number: self.quotation_number,
            status: self.status,
            is_expired,
            customer_key: self.customer_key,
            customer_name: self.customer_name,
            customer_phone: self.customer_phone,
            customer_address: self.customer_address,
            items: self.items,
            subtotal_cents: self.subtotal_cents,
            discount_type: self.discount_type,
            discount_value: self.discount_value,
            discount_cents: self.discount_cents,
            total_cents: self.total_cents,
            valid_until: self.valid_until,
            notes: self.notes,
            terms: self.terms,
            prepared_by: self.prepared_by,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
