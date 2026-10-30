use super::label_coverage_status::LabelCoverageStatus;
use crate::domain::inventory::Product;
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// One product with its label state. `status` is `None` for a product that
/// was never labelled and has nothing in stock (nothing to label).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelCoverageRow {
    pub product: Product,
    pub status: Option<LabelCoverageStatus>,
    pub last_labelled_at: Option<DateTime<Utc>>,
    /// Stickers of this product in its last print.
    pub last_labelled_copies: Option<u32>,
    /// Units entered after the last label that are still in stock.
    pub received_since_label: i64,
}
