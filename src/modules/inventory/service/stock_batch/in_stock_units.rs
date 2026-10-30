/// Units still on the shelf from one batch, and when that batch was entered.
/// What `labels` needs to tell which units arrived after a product's last label.
#[derive(Debug, Clone)]
pub(crate) struct InStockUnits {
    pub product_key: String,
    pub remaining_quantity: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
