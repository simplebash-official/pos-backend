//! Refund and discount analytics rows and responses.

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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscountCashierRow {
    pub cashier_id: String,
    pub cashier_name: String,
    pub discount_cents: i64,
    pub revenue_cents: i64,
    pub discount_rate_bps: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscountProductRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    pub name: String,
    pub discount_cents: i64,
    pub units_sold: i64,
}

// --- discounts --------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscountTypeRow {
    /// `percentage` | `fixed` | `none`.
    pub discount_type: String,
    pub invoice_count: u64,
    pub discount_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundAnalyticsResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub credit_note_count: u64,
    pub net_refund_cents: i64,
    pub refund_cash_cents: i64,
    pub balance_reduction_cents: i64,
    /// Net refunds as a share of period revenue, in basis points.
    pub refund_rate_bps: i64,
    pub exchange_count: u64,
    pub no_receipt_count: u64,
    pub manager_override_count: u64,
    pub by_reason: Vec<RefundReasonRow>,
    pub by_method: Vec<RefundMethodRow>,
    pub top_returned_products: Vec<RefundProductRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundMethodRow {
    pub method: String,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundProductRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    pub name: String,
    pub quantity: i64,
    pub amount_cents: i64,
}

// --- refunds ---------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundReasonRow {
    pub reason: String,
    pub credit_note_item_count: u64,
    pub amount_cents: i64,
}
