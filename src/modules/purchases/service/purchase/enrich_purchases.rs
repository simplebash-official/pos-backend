use crate::clients::db::Db;
use crate::core::error::AppResult;

use crate::domain::purchases::{ProductSummary, Purchase, SupplierSummary};
use crate::modules::inventory::service::product as inventory_product;
use crate::modules::purchases::model::PurchaseDocument;

use crate::modules::suppliers::service as supplier_service;

use std::collections::HashMap;

/// Attaches the live supplier and product summaries to purchase lines, with the same two batch
/// lookups for every caller (list, receipt, sync), so a line carries the same display data
/// however it is delivered. A reference that no longer resolves is left off, never faked.
pub(crate) async fn enrich_purchases(
    db: &Db,
    documents: Vec<PurchaseDocument>,
) -> AppResult<Vec<Purchase>> {
    if documents.is_empty() {
        return Ok(Vec::new());
    }

    let supplier_keys: Vec<String> = documents
        .iter()
        .map(|document| document.supplier_key.clone())
        .collect();
    let product_keys: Vec<String> = documents
        .iter()
        .map(|document| document.product_key.clone())
        .collect();

    let suppliers_by_key: HashMap<String, SupplierSummary> =
        supplier_service::get_suppliers_by_keys(db, &supplier_keys)
            .await?
            .into_iter()
            .map(|supplier| {
                (
                    supplier.key.clone(),
                    SupplierSummary {
                        id: supplier.id,
                        key: supplier.key,
                        name: supplier.name,
                        contact_person: supplier.contact_person,
                        primary_phone: supplier.primary_phone,
                    },
                )
            })
            .collect();
    let products_by_key: HashMap<String, ProductSummary> =
        inventory_product::get_products_by_keys(db, &product_keys)
            .await?
            .into_iter()
            .map(|product| {
                (
                    product.key.clone(),
                    ProductSummary {
                        id: product.id,
                        key: product.key,
                        sku: product.sku,
                        name: product.name,
                        category: product.category,
                        subcategory: product.subcategory,
                    },
                )
            })
            .collect();

    Ok(documents
        .into_iter()
        .map(|document| {
            let supplier = suppliers_by_key.get(&document.supplier_key).cloned();
            let product = products_by_key.get(&document.product_key).cloned();
            document.into_purchase(supplier, product)
        })
        .collect())
}
