use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategorySalesRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_key: Option<String>,
    pub category_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcategory_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcategory_name: Option<String>,
    pub units_sold: i64,
    pub revenue_cents: i64,
    pub discount_cents: i64,
    pub cogs_cents: i64,
    pub gross_profit_cents: i64,
    pub gross_margin_bps: i64,
}
