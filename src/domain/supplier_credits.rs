// Pure business types for supplier credits: an amount a supplier takes off what the shop owes,
// because goods were short or damaged or the price was wrong. No I/O, no Mongo/Axum types beyond
// serde/utoipa derives.
//
// A credit lowers the supplier's balance and, when it names a receipt, that receipt's amount
// due. It moves no stock: the shop receives only the units it accepts. A wrong credit is voided
// by a reversing entry, never edited or deleted.

use crate::domain::inventory::PaginationMeta;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Why the supplier gave credit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreditReason {
    /// Fewer units arrived than were billed.
    ShortDelivery,
    /// Goods arrived or turned out damaged.
    Damaged,
    /// The billed price was higher than agreed.
    PriceDifference,
    Other,
}

impl CreditReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            CreditReason::ShortDelivery => "short_delivery",
            CreditReason::Damaged => "damaged",
            CreditReason::PriceDifference => "price_difference",
            CreditReason::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "short_delivery" => Some(CreditReason::ShortDelivery),
            "damaged" => Some(CreditReason::Damaged),
            "price_difference" => Some(CreditReason::PriceDifference),
            "other" => Some(CreditReason::Other),
            _ => None,
        }
    }
}

/// A credit as returned to API clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierCredit {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key (`scr_...`).
    pub key: String,
    pub supplier_key: String,
    /// The receipt this credit is against, when it is against one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_key: Option<String>,
    pub amount_cents: i64,
    pub reason: CreditReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Set on a voiding entry: the credit it cancels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverses_key: Option<String>,
    /// Set when a voiding entry for this credit exists. Worked out when read, never stored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_by_key: Option<String>,
    pub recorded_by_user_id: String,
    pub recorded_by_name_snapshot: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

/// Body for `POST /supplier-credits`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSupplierCreditRequest {
    pub supplier_key: String,
    /// The receipt the credit is against; omitted for a credit on the supplier's account.
    #[serde(default)]
    pub receipt_key: Option<String>,
    pub amount_cents: i64,
    pub reason: CreditReason,
    #[serde(default)]
    pub note: Option<String>,
}

/// Query params for `GET /supplier-credits`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SupplierCreditListQuery {
    pub supplier_key: Option<String>,
    pub receipt_key: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

/// Paginated response for `GET /supplier-credits`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierCreditListResponse {
    pub items: Vec<SupplierCredit>,
    pub pagination: PaginationMeta,
}
