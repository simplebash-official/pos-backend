use crate::domain::reports::PaymentMethodBreakdown;

#[derive(Debug, Clone, Default)]
pub(crate) struct SalesSummaryAggregate {
    pub total_invoices: u64,
    pub gross_sales_cents: i64,
    pub discount_cents: i64,
    pub total_sales_cents: i64,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub payment_methods: PaymentMethodBreakdown,
}
