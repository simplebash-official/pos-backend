use super::quotation_item::QuotationItem;

use chrono::{DateTime, Utc};
use serde::Serialize;

/// A quotation as it travels between devices and the cloud (`GET /sync/pull`,
/// snapshots, pushes). Unlike the REST `Quotation` it carries the frozen shop
/// profile (a device that applies a quotation must be able to reprint it
/// exactly) and the sync metadata, and it has no `isExpired`: that is derived
/// from the date at read time and would otherwise count as a content change.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncQuotation {
    pub id: String,
    pub key: String,
    pub quotation_number: String,
    pub status: String,
    #[serde(default)]
    pub has_out_of_stock_items: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fulfilled_by_invoice_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fulfilled_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_address: Option<String>,
    pub items: Vec<QuotationItem>,
    pub subtotal_cents: i64,
    pub discount_type: String,
    pub discount_value: f64,
    pub discount_cents: i64,
    pub total_cents: i64,
    pub valid_until: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepared_by: Option<String>,
    pub shop_profile_snapshot: serde_json::Value,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
