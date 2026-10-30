use super::attach_price_tiers::attach_price_tiers;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::Product;

/// `attach_price_tiers` for a single product.
pub(crate) async fn with_price_tiers(db: &Db, product: Product) -> AppResult<Product> {
    let mut products = [product];
    attach_price_tiers(db, &mut products).await?;
    let [product] = products;
    Ok(product)
}
