// Derived-field recompute for the cloud (MongoDB) engine: stock, customer
// balances and an invoice's refund totals / per-line returned quantity /
// credit-sale status are recomputed from the ledger rows after every applied
// batch, exactly as `derived_sqlite` does on a device (formulas: sync v2 spec
// 1.3), so both engines converge on the ledger truth.
//
// One deliberate difference from the device engine: only the derived columns
// are written; `updated_at`, `version` and `updated_by_device` are left alone.
// The cloud reads `updated_at` for last-writer-wins, so bumping it here would
// let a recompute of stale ordinary columns beat a concurrent real edit.

use std::collections::BTreeSet;

use futures_util::TryStreamExt;
use mongodb::bson::{self, Bson, Document, doc, oid::ObjectId};

use crate::{
    clients::db::Db,
    core::error::{AppError, AppResult},
    domain::{
        billing::{InvoiceItem, InvoiceStatus},
        sync_v2::{ConflictKind, DerivedScope, NewConflict},
    },
    modules::{
        billing::service::credit_notes::line_match,
        sync::{apply_mongo::num_i64, cloud_store::coll},
    },
};

/// Recomputes every derived field touched by `scope`; returns the invariant
/// conflicts found (over-refund, negative stock).
pub(crate) async fn recompute_derived(
    db: &Db,
    scope: &DerivedScope,
) -> AppResult<Vec<NewConflict>> {
    let mut conflicts = Vec::new();
    let mut customer_keys: BTreeSet<String> = scope.customer_keys.clone();

    for invoice_key in &scope.invoice_keys {
        if let Some(customer_key) = recompute_invoice(db, invoice_key, &mut conflicts).await? {
            customer_keys.insert(customer_key);
        }
    }
    for customer_key in &customer_keys {
        recompute_customer(db, customer_key).await?;
    }
    for product_id in &scope.product_ids {
        recompute_product(db, product_id, &mut conflicts).await?;
    }
    Ok(conflicts)
}

/// `SUM(field)` over the documents of `collection` matching `filter`.
async fn sum_field(db: &Db, collection: &str, filter: Document, field: &str) -> AppResult<i64> {
    let mut cursor = coll(db, collection)?
        .aggregate(vec![
            doc! { "$match": filter },
            doc! { "$group": { "_id": Bson::Null, "total": { "$sum": format!("${field}") } } },
        ])
        .await?;
    Ok(match cursor.try_next().await? {
        Some(row) => num_i64(&row, "total").unwrap_or(0),
        None => 0,
    })
}

fn status_str(status: InvoiceStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Refund totals, per-line returned quantity and (for credit sales) the
/// payment-progress status of one invoice. Returns its customer key.
async fn recompute_invoice(
    db: &Db,
    key: &str,
    conflicts: &mut Vec<NewConflict>,
) -> AppResult<Option<String>> {
    let invoices = coll(db, "invoices")?.preserve_origin();
    let Some(invoice) = invoices.find_one(doc! { "key": key }).await? else {
        return Ok(None);
    };
    let total_cents = num_i64(&invoice, "total_cents").unwrap_or(0);
    let is_credit = invoice.get_bool("is_credit").unwrap_or(false);
    let status = invoice.get_str("status").unwrap_or_default().to_string();
    let old_refunded = num_i64(&invoice, "refunded_cents").unwrap_or(0);
    let old_count = num_i64(&invoice, "credit_note_count").unwrap_or(0);
    let customer_key = invoice.get_str("customer_key").ok().map(str::to_string);

    let old_items: Vec<InvoiceItem> = invoice
        .get("items")
        .cloned()
        .map(bson::deserialize_from_bson)
        .transpose()
        .map_err(|e| AppError::internal(format!("invoice items are unreadable: {e}")))?
        .unwrap_or_default();
    let mut items = old_items.clone();
    for item in &mut items {
        item.returned_quantity = 0;
    }

    let mut notes = coll(db, "credit_notes")?
        .find(doc! { "invoice_key": key, "status": { "$ne": "voided" } })
        .sort(doc! { "created_at": 1, "key": 1 })
        .await?;
    let mut refunded = 0i64;
    let mut count = 0i64;
    while let Some(note) = notes.try_next().await? {
        count += 1;
        refunded += num_i64(&note, "refund_cash_cents").unwrap_or(0)
            + num_i64(&note, "balance_reduction_cents").unwrap_or(0);
        let lines: Vec<Document> = note
            .get_array("returned_items")
            .map(|a| a.iter().filter_map(|b| b.as_document().cloned()).collect())
            .unwrap_or_default();
        let sole = items.len() == 1 && lines.len() == 1;
        for line in &lines {
            let quantity = num_i64(line, "quantity").unwrap_or(0);
            if let Some(idx) = line_match(
                &items,
                line.get_str("productKey").ok(),
                line.get_str("sourceTicketKey").ok(),
                line.get_str("name").ok(),
                sole,
            ) {
                items[idx].returned_quantity += quantity;
            }
        }
    }

    if refunded > total_cents {
        conflicts.push(NewConflict {
            kind: ConflictKind::OverRefund,
            resource: "invoices".to_string(),
            entity_key: key.to_string(),
            detail: serde_json::json!({
                "invoiceKey": key,
                "refundedCents": refunded,
                "totalCents": total_cents,
            }),
        });
    }

    // Credit sales settle through payments; voided / closed are sticky.
    let mut new_status = status.clone();
    if is_credit && matches!(status.as_str(), "pending" | "partially_paid" | "paid") {
        let paid = sum_field(db, "payments", doc! { "invoice_key": key }, "amount_cents").await?;
        new_status = status_str(InvoiceStatus::from_payment_progress(total_cents, paid));
    }

    let items_changed = serde_json::to_value(&items)? != serde_json::to_value(&old_items)?;
    if items_changed || refunded != old_refunded || count != old_count || new_status != status {
        let items_bson = bson::serialize_to_bson(&items)
            .map_err(|e| AppError::internal(format!("cannot store invoice items: {e}")))?;
        invoices
            .update_one(
                doc! { "key": key },
                doc! { "$set": {
                    "items": items_bson,
                    "refunded_cents": refunded,
                    "credit_note_count": count,
                    "status": new_status,
                }},
            )
            .await?;
    }
    Ok(customer_key)
}

/// `total_purchases_cents` and `outstanding_balance_cents` from the invoice /
/// payment / credit-note ledger, never from incremental deltas.
async fn recompute_customer(db: &Db, key: &str) -> AppResult<()> {
    let customers = coll(db, "customers")?.preserve_origin();
    let Some(customer) = customers
        .find_one(doc! { "key": key, "deleted_at": Bson::Null })
        .await?
    else {
        return Ok(());
    };
    let old_outstanding = num_i64(&customer, "outstanding_balance_cents").unwrap_or(0);
    let old_purchases = num_i64(&customer, "total_purchases_cents").unwrap_or(0);

    let sales = sum_field(
        db,
        "invoices",
        doc! { "customer_key": key, "status": { "$ne": "voided" } },
        "total_cents",
    )
    .await?;
    let refunds = sum_field(
        db,
        "credit_notes",
        doc! { "customer_key": key, "status": { "$ne": "voided" } },
        "refund_cash_cents",
    )
    .await?;

    let mut credit_invoices = coll(db, "invoices")?
        .find(doc! { "customer_key": key, "is_credit": true, "status": { "$ne": "voided" } })
        .await?;
    let mut outstanding = 0i64;
    while let Some(invoice) = credit_invoices.try_next().await? {
        let paid = sum_field(
            db,
            "payments",
            doc! { "invoice_key": invoice.get_str("key").unwrap_or_default() },
            "amount_cents",
        )
        .await?;
        outstanding += num_i64(&invoice, "total_cents").unwrap_or(0) - paid;
    }

    let purchases = sales - refunds;
    if outstanding != old_outstanding || purchases != old_purchases {
        customers
            .update_one(
                doc! { "key": key },
                doc! { "$set": {
                    "outstanding_balance_cents": outstanding,
                    "total_purchases_cents": purchases,
                }},
            )
            .await?;
    }
    Ok(())
}

/// `stock_quantity` = SUM of the product's movements. A product that becomes
/// negative raises `NEGATIVE_STOCK` once, at the transition.
async fn recompute_product(
    db: &Db,
    product_id: &str,
    conflicts: &mut Vec<NewConflict>,
) -> AppResult<()> {
    let Ok(oid) = ObjectId::parse_str(product_id) else {
        return Ok(());
    };
    let products = coll(db, "products")?.preserve_origin();
    let Some(product) = products.find_one(doc! { "_id": oid }).await? else {
        return Ok(());
    };
    let old = num_i64(&product, "stock_quantity").unwrap_or(0);
    let total = sum_field(
        db,
        "stock_movements",
        doc! { "product_id": oid },
        "quantity_delta",
    )
    .await?;
    if total == old {
        return Ok(());
    }
    products
        .update_one(
            doc! { "_id": oid },
            doc! { "$set": { "stock_quantity": total } },
        )
        .await?;
    if total < 0 && old >= 0 {
        conflicts.push(NewConflict {
            kind: ConflictKind::NegativeStock,
            resource: "products".to_string(),
            entity_key: product.get_str("key").unwrap_or_default().to_string(),
            detail: serde_json::json!({
                "productKey": product.get_str("key").unwrap_or_default(),
                "quantity": total,
            }),
        });
    }
    Ok(())
}
