//! The warranty a product (or a category's template for its products) comes with.

use crate::core::error::{AppError, AppResult};
use pos_calc_core::warranty::WarrantyCoverage;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Who stands behind a warranty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WarrantyType {
    /// The shop repairs or replaces the item itself.
    Shop,
    /// The maker or distributor does; the shop passes the claim on.
    Manufacturer,
    /// The shop handles it first and the maker covers it after.
    Both,
}

impl WarrantyType {
    /// The name this value has on the wire and in storage.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            WarrantyType::Shop => "shop",
            WarrantyType::Manufacturer => "manufacturer",
            WarrantyType::Both => "both",
        }
    }
}

/// The warranty an item comes with. Stored inside its product (and, as a template, its
/// category) exactly as sent, so the camelCase field names are also the stored names.
///
/// `coverage` `none` carries no `warrantyType` or `terms`; any other coverage needs a
/// `warrantyType` (checked by `validated`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductWarranty {
    pub coverage: WarrantyCoverage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_type: Option<WarrantyType>,
    /// What is covered and what is not, printed with the warranty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms: Option<String>,
}

impl ProductWarranty {
    /// No warranty at all.
    #[must_use]
    pub fn none() -> Self {
        ProductWarranty {
            coverage: WarrantyCoverage::None,
            warranty_type: None,
            terms: None,
        }
    }

    /// Whether this item is covered at all.
    #[must_use]
    pub fn is_covered(&self) -> bool {
        self.coverage != WarrantyCoverage::None
    }

    /// Checks the profile and trims its terms: a covered item needs a type, an uncovered one
    /// carries neither type nor terms, terms are 1 to 1000 characters, and the period is in range.
    ///
    /// # Errors
    ///
    /// A validation error saying what to change.
    pub fn validated(self) -> AppResult<ProductWarranty> {
        self.coverage.validate()?;
        let covered = self.is_covered();
        let terms = match self.terms {
            None => None,
            Some(terms) => {
                let trimmed = terms.trim();
                if trimmed.is_empty() {
                    return Err(AppError::validation(
                        "Warranty terms cannot be blank. Leave them out instead.",
                    ));
                }
                if trimmed.chars().count() > TERMS_MAX {
                    return Err(AppError::validation(format!(
                        "Warranty terms can be at most {TERMS_MAX} characters."
                    )));
                }
                Some(trimmed.to_string())
            }
        };
        match (covered, self.warranty_type, &terms) {
            (true, None, _) => Err(AppError::validation(
                "Choose who gives the warranty: the shop, the manufacturer, or both.",
            )),
            (false, Some(_), _) | (false, None, Some(_)) => Err(AppError::validation(
                "An item with no warranty cannot have a warranty type or terms.",
            )),
            (_, warranty_type, _) => Ok(ProductWarranty {
                coverage: self.coverage,
                warranty_type,
                terms,
            }),
        }
    }
}

/// The longest warranty terms text.
const TERMS_MAX: usize = 1000;
