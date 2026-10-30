use crate::core::error::AppError;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How a product's `barcode` was populated. `None` on `Product` whenever
/// `barcode` itself is `None` — a product may be created with no barcode at
/// all. A barcode set or corrected later via `PUT /products/{id}` is always
/// recorded as `Manual`, regardless of what it was before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BarcodeSource {
    Generated,
    Manual,
}

impl BarcodeSource {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Manual => "manual",
        }
    }
}

impl std::str::FromStr for BarcodeSource {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "generated" => Ok(Self::Generated),
            "manual" => Ok(Self::Manual),
            _ => Err(AppError::validation(format!("Invalid barcode source: {s}"))),
        }
    }
}
