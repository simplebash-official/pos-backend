use super::discount_cashier_row::DiscountCashierRow;
use super::discount_product_row::DiscountProductRow;
use super::discount_type_row::DiscountTypeRow;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscountAnalyticsResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_discount_cents: i64,
    pub gross_before_discount_cents: i64,
    pub discount_rate_bps: i64,
    pub invoices_with_discount: u64,
    pub invoice_count: u64,
    pub order_level_discount_cents: i64,
    pub line_level_discount_cents: i64,
    pub by_type: Vec<DiscountTypeRow>,
    pub by_cashier: Vec<DiscountCashierRow>,
    pub top_discounted_products: Vec<DiscountProductRow>,
}
