use super::quotation_item::QuotationItem;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A persisted quotation as returned by the API. The frozen shop-profile
/// snapshot is intentionally not part of it (it can carry a base64 logo);
/// only the PDF builder reads it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Quotation {
    pub id: String,
    pub key: String,
    /// Human-facing number, e.g. `QUO-0001`.
    pub quotation_number: String,
    /// `active`, `fulfilled`, or `cancelled`.
    pub status: String,
    /// True when still `active` but `validUntil` is in the past.
    pub is_expired: bool,
    /// True when this quotation was created with items that were out of stock.
    #[serde(default)]
    pub has_out_of_stock_items: bool,
    /// The invoice that converted/fulfilled this quotation, if fulfilled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fulfilled_by_invoice_key: Option<String>,
    /// When this quotation was converted to a sale.
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
