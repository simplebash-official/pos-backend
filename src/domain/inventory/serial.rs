//! Serial numbers of serialised products.

use super::product::Product;
use crate::core::error::AppError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

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
    /// Key of the credit note that most recently returned this unit, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_note_key: Option<String>,
    /// Timestamp when this serial record was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when this serial record was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version.
    pub version: i64,
}

/// Query params for `GET /products/{key}/serials`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ProductSerialListQuery {
    /// Filter by lifecycle status (e.g. `in_stock` for a sale-time picker).
    pub status: Option<SerialStatus>,
}

/// Response for `GET /products/{key}/serials`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductSerialListResponse {
    /// Matching serial units for the requested product.
    pub items: Vec<ProductSerial>,
}

/// What scanning a serial number resolves to: the unit, its product and the
/// price it sells at (its own batch's selling price). Admin-only cost fields are
/// stripped from `product` by the route, like every other product read.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SerialLookup {
    pub serial: ProductSerial,
    pub product: Product,
    pub selling_price_cents: i64,
}

/// Lifecycle of one physical serialized unit of a product. A unit starts
/// `InStock` when received on a purchase, becomes `Sold` at checkout, and
/// on a later credit-note return moves to exactly one of `ReturnedResalable`
/// / `ReturnedFaulty` / `UnderWarrantyClaim` / `WrittenOff` depending on the
/// return line's `condition`/`disposition` (see
/// `billing::service::credit_notes::create_credit_note`) — this is
/// per-unit history, kept separate from the product's aggregate
/// `stock_quantity` so an individual unit can be traced end to end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SerialStatus {
    InStock,
    Sold,
    ReturnedResalable,
    ReturnedFaulty,
    UnderWarrantyClaim,
    WrittenOff,
}

impl SerialStatus {
    #[must_use]
    /// The name this value has on the wire and in storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InStock => "in_stock",
            Self::Sold => "sold",
            Self::ReturnedResalable => "returned_resalable",
            Self::ReturnedFaulty => "returned_faulty",
            Self::UnderWarrantyClaim => "under_warranty_claim",
            Self::WrittenOff => "written_off",
        }
    }
}

impl std::str::FromStr for SerialStatus {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "in_stock" => Ok(Self::InStock),
            "sold" => Ok(Self::Sold),
            "returned_resalable" => Ok(Self::ReturnedResalable),
            "returned_faulty" => Ok(Self::ReturnedFaulty),
            "under_warranty_claim" => Ok(Self::UnderWarrantyClaim),
            "written_off" => Ok(Self::WrittenOff),
            _ => Err(AppError::validation(format!("Invalid serial status: {s}"))),
        }
    }
}
