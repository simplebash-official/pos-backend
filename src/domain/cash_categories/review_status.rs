use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Whether a manager has looked at a category yet. Categories a cashier makes on
/// the spot show immediately with a "New" badge and wait here for review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Approved,
    NeedsReview,
}

impl ReviewStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewStatus::Approved => "approved",
            ReviewStatus::NeedsReview => "needs_review",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "approved" => Some(ReviewStatus::Approved),
            "needs_review" => Some(ReviewStatus::NeedsReview),
            _ => None,
        }
    }
}
