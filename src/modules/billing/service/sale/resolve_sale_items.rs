use super::resolve_sale_item::resolve_sale_item;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::domain::billing::{CreateSaleItemRequest, InvoiceItem};
use crate::domain::inventory::Product;

use crate::modules::inventory;

use std::collections::HashMap;

/// Resolves each request-side item into a persisted-shape `InvoiceItem`,
/// pulling `name`/`sku`/`unitPriceCents` (and, for repair/print, the
/// assigned employee) from the referenced product/ticket rather than
/// trusting the client — mirrors the customer-key resolution just above it
/// in `complete_sale`. Also returns the resolved `Product`s keyed by
/// `product_key` so the post-commit stock-decrement loop doesn't have to
/// look them up a second time. An item whose `productKey`/`sourceTicketKey`
/// doesn't resolve fails the whole request (via `?`) before any write,
/// exactly like a bad `customerKey` already does.
///
/// Retail products are fetched in one batched `$in` query up front (see
/// `inventory::service::product::get_products_by_keys`) rather than one
/// `get_product_by_key` round trip per item — a large cart previously paid 3
/// sequential Mongo round trips per retail line just for this lookup.
/// Remaining per-item work (repair/print ticket lookups) runs concurrently
/// via `try_join_all` instead of a sequential loop, so the whole resolution
/// step costs roughly one round trip's worth of latency regardless of cart
/// size, not N.
pub(crate) async fn resolve_sale_items(
    db: &Db,
    items: &[CreateSaleItemRequest],
) -> AppResult<(Vec<InvoiceItem>, HashMap<String, Product>)> {
    // Each line's batches are planned against current stock independently,
    // so two lines at the same product+price would both plan on the same
    // units. The cart keeps one line per price; refuse anything else.
    let mut seen_tiers = std::collections::HashSet::new();
    for item in items {
        if let (Some(product_key), Some(price)) = (&item.product_key, item.selling_price_cents)
            && !seen_tiers.insert((product_key.as_str(), price))
        {
            return Err(AppError::validation(
                "The same product at the same price appears on two lines — combine them into one",
            ));
        }
    }
    let retail_product_keys: Vec<String> = items
        .iter()
        .filter(|item| item.source_type == "retail")
        .filter_map(|item| item.product_key.clone())
        .collect();
    let products: HashMap<String, Product> =
        inventory::service::product::get_products_by_keys(db, &retail_product_keys)
            .await?
            .into_iter()
            .map(|product| (product.key.clone(), product))
            .collect();

    let resolved: Vec<InvoiceItem> = futures_util::future::try_join_all(
        items
            .iter()
            .map(|item| resolve_sale_item(db, item, &products)),
    )
    .await?;

    Ok((resolved, products))
}
