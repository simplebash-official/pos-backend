use super::daily_sales_report_response::DailySalesReportResponse;
use super::discount_analytics_response::DiscountAnalyticsResponse;
use super::refund_analytics_response::RefundAnalyticsResponse;
use super::sales_by_category_response::SalesByCategoryResponse;
use super::top_products_response::TopProductsResponse;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Sales section feed bundle.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesFeedData {
    pub daily_sales: DailySalesReportResponse,
    pub sales_by_category: SalesByCategoryResponse,
    pub discounts: DiscountAnalyticsResponse,
    pub refunds: RefundAnalyticsResponse,
    pub top_products: TopProductsResponse,
}
