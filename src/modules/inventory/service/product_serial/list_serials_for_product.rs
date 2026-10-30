use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::{ProductSerial, SerialStatus};
use crate::modules::inventory::model::ProductSerialDocument;
use crate::modules::inventory::repository;

/// Lists a product's serial units, optionally filtered by lifecycle status.
/// Backs `GET /products/{key}/serials`, used both by a sale-time "pick a
/// unit to sell" picker (`status=in_stock`) and a return-time lookup.
pub(crate) async fn list_serials_for_product(
    db: &Db,
    product_key: &str,
    status: Option<SerialStatus>,
) -> AppResult<Vec<ProductSerial>> {
    let status_str = status.map(|s| {
        serde_json::to_value(s)
            .expect("a serial status serializes to JSON")
            .as_str()
            .expect("a serial status serializes to a string")
            .to_string()
    });
    let documents = repository::product_serial::find_serials_for_product(
        db,
        product_key,
        status_str.as_deref(),
    )
    .await?;
    Ok(documents
        .into_iter()
        .map(ProductSerialDocument::into_product_serial)
        .collect())
}
