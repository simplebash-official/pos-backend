use crate::modules::inventory::model::ProductDocument;

use std::collections::HashMap;

#[derive(Debug)]
pub(crate) struct OverviewAggregateResult {
    pub total_items: u64,
    pub low_stock_alerts: u64,
    pub category_counts: HashMap<String, u64>,
    pub subcategory_data: HashMap<String, (u64, Vec<ProductDocument>)>,
}
