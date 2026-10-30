use crate::domain::sync_v2::DerivedScope;

use crate::modules::sync::resources::ResourceSpec;

use mongodb::bson::Document;
use serde_json::Value;

/// Adds the derived-scope entries a written change implies.
pub(super) fn note_scope(
    spec: &ResourceSpec,
    payload: Option<&Value>,
    key: &str,
    doc: &Document,
    scope: &mut DerivedScope,
) {
    let text = |name: &str| payload.and_then(|p| p.get(name)).and_then(Value::as_str);
    match spec.name {
        "products" => {
            if let Ok(id) = doc.get_object_id("_id") {
                scope.product_ids.insert(id.to_hex());
            }
        }
        "stockMovements" => {
            if let Some(id) = text("productId") {
                scope.product_ids.insert(id.to_string());
            }
            if let Some(batch) = text("batchKey") {
                scope.batch_keys.insert(batch.to_string());
            }
        }
        "stockBatches" => {
            scope.batch_keys.insert(key.to_string());
        }
        "customers" => {
            scope.customer_keys.insert(key.to_string());
        }
        "invoices" => {
            scope.invoice_keys.insert(key.to_string());
            if let Some(customer) = text("customerKey") {
                scope.customer_keys.insert(customer.to_string());
            }
        }
        "payments" => {
            if let Some(invoice) = text("invoiceKey") {
                scope.invoice_keys.insert(invoice.to_string());
            }
        }
        "creditNotes" => {
            if let Some(invoice) = text("invoiceKey") {
                scope.invoice_keys.insert(invoice.to_string());
            }
            if let Some(customer) = text("customerKey") {
                scope.customer_keys.insert(customer.to_string());
            }
        }
        _ => {}
    }
}
