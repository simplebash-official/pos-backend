use crate::clients::db::Db;
use crate::core::error::AppResult;

use crate::modules::billing::service as billing_service;
use crate::modules::billing::service::{
    credit_notes as billing_credit_notes_service, payments as billing_payments_service,
};
use crate::modules::customers::service as customers_service;
use crate::modules::employees::service as employees_service;
use crate::modules::inventory::service::{
    category as inventory_category, product as inventory_product,
    product_serial as inventory_product_serial, stock as inventory_stock,
};
use crate::modules::print_jobs::service as print_jobs_service;
use crate::modules::purchases::service::{
    credit as purchases_credit_service, purchase as purchases_service,
    receipt as purchases_receipt_service, settlement as purchases_settlement_service,
};
use crate::modules::quotations::service as quotations_service;
use crate::modules::repairs::service as repairs_service;
use crate::modules::supplier_products::service::link as supplier_products_service;
use crate::modules::suppliers::service as suppliers_service;

use mongodb::bson::Document;
use serde_json::Value;

/// Turns a page of raw documents into the same DTOs the REST read endpoints
/// return.
///
/// This is the whole point of the module. The client mirrors both feeds into
/// one local table, so a row arriving by delta must be indistinguishable
/// from the same row arriving in a snapshot — same camelCase field names,
/// same resolved display fields, no BSON `$oid`/`$date` wrappers. Serializing
/// the raw `Document` instead would write structurally different rows for the
/// same entity depending only on which feed happened to deliver it.
pub(crate) async fn hydrate(
    db: &Db,
    resource: &str,
    documents: Vec<Document>,
) -> AppResult<Vec<Value>> {
    let values = match resource {
        "products" => to_values(inventory_product::hydrate_sync_documents(db, documents).await?)?,
        "categories" => {
            to_values(inventory_category::hydrate_sync_documents(db, documents).await?)?
        }
        "suppliers" => to_values(suppliers_service::hydrate_sync_documents(documents)?)?,
        "supplierProducts" | "supplier_products" => to_values(
            supplier_products_service::hydrate_sync_documents(documents)?,
        )?,
        "purchases" => to_values(purchases_service::hydrate_sync_documents(db, documents).await?)?,
        "purchaseReceipts" | "purchase_receipts" => to_values(
            purchases_receipt_service::hydrate_sync_documents(documents)?,
        )?,
        "receiptSettlements" | "receipt_settlements" => to_values(
            purchases_settlement_service::hydrate_sync_documents(documents)?,
        )?,
        "supplierCredits" | "supplier_credits" => {
            to_values(purchases_credit_service::hydrate_sync_documents(documents)?)?
        }
        "receiptCharges" | "receipt_charges" => to_values(
            purchases_receipt_service::hydrate_charge_documents(documents)?,
        )?,
        // The expense ledger has no read-time enrichment on its stored rows, so the wire shape is
        // the stored fields in camelCase.
        "expenses" | "expensePayments" | "expense_payments" => {
            documents.into_iter().map(stored_fields_to_json).collect()
        }
        "stockMovements" | "stock_movements" => {
            to_values(inventory_stock::hydrate_sync_documents(documents)?)?
        }
        "stockBatches" | "stock_batches" => to_values(
            crate::modules::inventory::service::stock_batch::hydrate_sync_documents(documents)?,
        )?,
        "customers" => to_values(customers_service::hydrate_sync_documents(documents)?)?,
        // Unlike every other arm here, `employees::hydrate_sync_documents` is
        // async and takes `db` — it enriches each row with a live-resolved
        // `login` summary, the same cross-module lookup a REST read performs
        // (see `modules::employees::service::hydrate_sync_documents`).
        "employees" => to_values(employees_service::hydrate_sync_documents(db, documents).await?)?,
        "repairs" => to_values(repairs_service::hydrate_sync_documents(documents)?)?,
        "printJobs" | "print_jobs" => {
            to_values(print_jobs_service::hydrate_sync_documents(documents)?)?
        }
        "invoices" => to_values(billing_service::hydrate_sync_documents(documents)?)?,
        "payments" => to_values(billing_payments_service::hydrate_sync_documents(documents)?)?,
        "creditNotes" => to_values(billing_credit_notes_service::hydrate_sync_documents(
            documents,
        )?)?,
        "quotations" => to_values(quotations_service::hydrate_sync_documents(documents)?)?,
        "productSerials" => {
            to_values(inventory_product_serial::hydrate_sync_documents(documents)?)?
        }
        // Sync transport only: the record carries the password hash (see
        // `users::sync_payload`). `users` is not in `SYNCABLE`, so the legacy
        // `/sync/changes` feed can never reach this arm.
        "users" => to_values(crate::modules::users::sync_payload::hydrate_sync_documents(
            documents,
        )?)?,
        "registerSessions" | "register_sessions" => to_values(
            crate::modules::sessions::service::hydrate_session_sync_documents(documents)?,
        )?,
        "cashMovements" | "cash_movements" => to_values(
            crate::modules::sessions::service::hydrate_movement_sync_documents(db, documents)
                .await?,
        )?,
        _ => Vec::new(),
    };
    Ok(values)
}

/// A stored document as the camelCase JSON a device applies: `_id` becomes `id`, dates become
/// RFC 3339 text, and the tenant marker (`tenant_id`) never leaves the cloud.
fn stored_fields_to_json(document: Document) -> Value {
    fn camel(snake: &str) -> String {
        let mut out = String::with_capacity(snake.len());
        let mut upper = false;
        for c in snake.chars() {
            if c == '_' {
                upper = true;
            } else if upper {
                out.extend(c.to_uppercase());
                upper = false;
            } else {
                out.push(c);
            }
        }
        out
    }
    fn convert(value: mongodb::bson::Bson) -> Value {
        use mongodb::bson::Bson;
        match value {
            Bson::ObjectId(id) => Value::String(id.to_hex()),
            Bson::DateTime(date) => Value::String(date.to_chrono().to_rfc3339()),
            Bson::Document(inner) => Value::Object(
                inner
                    .into_iter()
                    .map(|(k, v)| (camel(&k), convert(v)))
                    .collect(),
            ),
            Bson::Array(items) => Value::Array(items.into_iter().map(convert).collect()),
            Bson::String(text) => Value::String(text),
            Bson::Boolean(flag) => Value::Bool(flag),
            Bson::Int32(n) => Value::from(n),
            Bson::Int64(n) => Value::from(n),
            Bson::Double(n) => Value::from(n),
            // The ledger rows hold only text, numbers, booleans, dates and ids; anything else
            // would be a field this conversion has not been taught.
            _ => Value::Null,
        }
    }
    Value::Object(
        document
            .into_iter()
            .filter(|(key, _)| key != crate::core::tenancy::TENANT_FIELD)
            .map(|(key, value)| {
                let name = if key == "_id" {
                    "id".to_string()
                } else {
                    camel(&key)
                };
                (name, convert(value))
            })
            .collect(),
    )
}

fn to_values<T: serde::Serialize>(items: Vec<T>) -> AppResult<Vec<Value>> {
    items
        .into_iter()
        .map(|item| serde_json::to_value(item).map_err(Into::into))
        .collect()
}
