// ---------------------------------------------------------------------------
// sales by category
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct CategoryAggRow {
    pub category_key: Option<String>,
    pub category_name: String,
    pub subcategory_key: Option<String>,
    pub subcategory_name: Option<String>,
    pub units_sold: i64,
    pub revenue_cents: i64,
    pub discount_cents: i64,
    pub cogs_cents: i64,
}
