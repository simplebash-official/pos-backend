use crate::core::error::{AppError, AppResult};
use crate::domain::warranties::{
    Warranty, WarrantyCoverage, WarrantyEvent, WarrantyEventKind, WarrantySourceType,
    WarrantyState, WarrantyStatus, WarrantyType,
};
use chrono::NaiveDate;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use pos_calc_core::warranty::{self, WarrantyClock};
use serde::{Deserialize, Serialize};

/// How a warranty is stored. Field names match the API's (snake_case of its camelCase), which
/// is what lets the sync codec carry it between a desktop and the cloud unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarrantyDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub warranty_number: String,
    pub source_type: WarrantySourceType,
    pub invoice_key: String,
    pub invoice_number: String,
    pub line_index: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_item_key: Option<String>,
    pub item_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    pub quantity: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    /// The last nine digits of `customer_phone`, so `0771234567` and `+94 77 123 4567` match
    /// the same lookup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone_tail: Option<String>,
    pub coverage: WarrantyCoverage,
    pub warranty_type: WarrantyType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms: Option<String>,
    pub unit_price_cents: i64,
    /// `YYYY-MM-DD`, shop-local.
    pub starts_on: String,
    /// `YYYY-MM-DD`; absent for lifetime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_on: Option<String>,
    pub status: WarrantyStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub void_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaces_warranty_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_by_warranty_key: Option<String>,
    pub claim_count: i64,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// Parses a stored `YYYY-MM-DD` day.
pub(crate) fn stored_day(field: &str, text: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| AppError::internal(format!("stored warranty {field} `{text}` is not a date")))
}

impl WarrantyDocument {
    /// The API shape, with where it stands on `today` (shop-local).
    pub fn into_warranty(self, today: NaiveDate) -> AppResult<Warranty> {
        let ends_on = self
            .ends_on
            .as_deref()
            .map(|text| stored_day("ends_on", text))
            .transpose()?;
        let state = match self.status {
            WarrantyStatus::Void => WarrantyState::Void,
            WarrantyStatus::UnderClaim => WarrantyState::UnderClaim,
            WarrantyStatus::Active => match warranty::clock(ends_on, today) {
                WarrantyClock::Active => WarrantyState::Active,
                WarrantyClock::ExpiringSoon => WarrantyState::ExpiringSoon,
                WarrantyClock::Expired => WarrantyState::Expired,
            },
        };
        Ok(Warranty {
            id: self
                .id
                .ok_or_else(|| AppError::internal("persisted warranty must have an id"))?
                .to_hex(),
            key: self.key,
            warranty_number: self.warranty_number,
            source_type: self.source_type,
            invoice_key: self.invoice_key,
            invoice_number: self.invoice_number,
            line_index: self.line_index,
            product_key: self.product_key,
            repair_key: self.repair_key,
            repair_item_key: self.repair_item_key,
            item_name: self.item_name,
            serial_number: self.serial_number,
            quantity: self.quantity,
            customer_key: self.customer_key,
            customer_name: self.customer_name,
            customer_phone: self.customer_phone,
            customer_phone_tail: self.customer_phone_tail,
            coverage: self.coverage,
            warranty_type: self.warranty_type,
            terms: self.terms,
            unit_price_cents: self.unit_price_cents,
            starts_on: self.starts_on,
            ends_on: self.ends_on,
            status: self.status,
            void_reason: self.void_reason,
            replaces_warranty_key: self.replaces_warranty_key,
            replaced_by_warranty_key: self.replaced_by_warranty_key,
            claim_count: self.claim_count,
            state,
            days_remaining: ends_on.map(|end| warranty::days_remaining(end, today)),
            version: self.version,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            updated_by_device: self.updated_by_device,
        })
    }
}

/// How one activity-log line is stored. Append-only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarrantyEventDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub warranty_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_key: Option<String>,
    pub kind: WarrantyEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub actor_user_id: String,
    pub actor_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by_name: Option<String>,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl WarrantyEventDocument {
    /// The API shape.
    pub fn into_event(self) -> AppResult<WarrantyEvent> {
        Ok(WarrantyEvent {
            id: self
                .id
                .ok_or_else(|| AppError::internal("persisted warranty event must have an id"))?
                .to_hex(),
            key: self.key,
            warranty_key: self.warranty_key,
            claim_key: self.claim_key,
            kind: self.kind,
            from_status: self.from_status,
            to_status: self.to_status,
            note: self.note,
            actor_user_id: self.actor_user_id,
            actor_name: self.actor_name,
            approved_by_user_id: self.approved_by_user_id,
            approved_by_name: self.approved_by_name,
            version: self.version,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            updated_by_device: self.updated_by_device,
        })
    }
}
