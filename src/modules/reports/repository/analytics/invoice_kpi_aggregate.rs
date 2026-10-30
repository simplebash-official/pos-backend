// ---------------------------------------------------------------------------
// KPI aggregate (one facet over `invoices`)
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct InvoiceKpiAggregate {
    pub invoice_count: u64,
    pub gross_sales_cents: i64,
    pub discount_cents: i64,
    pub net_sales_cents: i64,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub retail_items_sold: i64,
    pub retail_cogs_cents: i64,
    /// Retail revenue on lines that carried a `unitCostCents` snapshot.
    pub retail_revenue_with_cost_cents: i64,
}
