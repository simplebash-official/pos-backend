use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Example values a design is previewed with before it is saved.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelSample {
    pub name: String,
    pub sku: String,
    pub category: String,
    pub price_cents: i64,
    pub code: String,
}
