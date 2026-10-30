use crate::core::error::AppError;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
