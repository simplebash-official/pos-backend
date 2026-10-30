use crate::core::error::AppError;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Where a stock batch's units came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BatchSource {
    /// A supplier purchase receipt (`purchase_key` is set).
    Purchase,
    /// Stock a product was created with, or held before batches existed.
    Opening,
    /// A spreadsheet import row.
    Import,
    /// A manual stock-in adjustment.
    Adjustment,
}

impl BatchSource {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Purchase => "purchase",
            Self::Opening => "opening",
            Self::Import => "import",
            Self::Adjustment => "adjustment",
        }
    }
}

impl std::str::FromStr for BatchSource {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "purchase" => Ok(Self::Purchase),
            "opening" => Ok(Self::Opening),
            "import" => Ok(Self::Import),
            "adjustment" => Ok(Self::Adjustment),
            _ => Err(AppError::validation(format!("Invalid batch source: {s}"))),
        }
    }
}
