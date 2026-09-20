// Derived-field recompute for the SQLite (device) engine. Sync never carries
// these columns: stock, customer balances and an invoice's refund totals /
// credit-sale status are recomputed from the ledger rows (movements, invoices,
// payments, credit notes) after every applied batch, so replaying the same
// batch - or merging concurrent edits - always converges on the ledger truth.
//
// Runs inside the applier's transaction (`sync_state.applying = 1`), so nothing
// it writes is enqueued to the outbox. Formulas are pinned in the sync v2 spec
// section 1.3; the Mongo engine implements identical semantics.

use std::collections::BTreeSet;

use chrono::Utc;
use sqlx::{Row, SqliteConnection};

use crate::{
    core::{error::AppResult, id::generate_id},
    domain::{
        billing::{InvoiceItem, InvoiceStatus},
        sync_v2::{ConflictKind, DerivedScope, NewConflict},
    },
    modules::billing::service::credit_notes::line_match,
};

/// Recomputes every derived field touched by `scope` and returns the
/// invariant conflicts found (over-refund, negative stock).
pub async fn recompute(
    conn: &mut SqliteConnection,
    scope: &DerivedScope,
) -> AppResult<Vec<NewConflict>> {
    let mut conflicts = Vec::new();
    let mut customer_keys: BTreeSet<String> = scope.customer_keys.clone();

    for invoice_key in &scope.invoice_keys {
        if let Some(customer_key) = recompute_invoice(conn, invoice_key, &mut conflicts).await? {
            customer_keys.insert(customer_key);
        }
    }
    for customer_key in &customer_keys {
        recompute_customer(conn, customer_key).await?;
    }
    for product_id in &scope.product_ids {
        recompute_product(conn, product_id, &mut conflicts).await?;
    }
    Ok(conflicts)
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn status_str(status: InvoiceStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Refund totals, per-line returned quantity and (for credit sales) the
/// payment-progress status of one invoice. Returns its customer key so the
/// caller can refresh that customer's balances.
async fn recompute_invoice(
    conn: &mut SqliteConnection,
    key: &str,
    conflicts: &mut Vec<NewConflict>,
) -> AppResult<Option<String>> {
    let Some(row) = sqlx::query(
        "SELECT total_cents, is_credit, status, items, refunded_cents, credit_note_count, customer_key \
         FROM invoices WHERE key = ? AND deleted_at IS NULL",
    )
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };

    let total_cents: i64 = row.get("total_cents");
    let is_credit: i64 = row.get("is_credit");
    let status: String = row.get("status");
    let items_json: String = row.get("items");
    let old_refunded: i64 = row.get("refunded_cents");
    let old_count: i64 = row.get("credit_note_count");
    let customer_key: Option<String> = row.get("customer_key");

    let old_items: Vec<InvoiceItem> = serde_json::from_str(&items_json).unwrap_or_default();
    let mut items = old_items.clone();
    for item in &mut items {
        item.returned_quantity = 0;
    }

    let notes = sqlx::query(
        "SELECT returned_items, refund_cash_cents, balance_reduction_cents FROM credit_notes \
         WHERE invoice_key = ? AND status != 'voided' AND deleted_at IS NULL ORDER BY created_at, key",
    )
    .bind(key)
    .fetch_all(&mut *conn)
    .await?;

    let mut refunded = 0i64;
    for note in &notes {
        refunded += note.get::<i64, _>("refund_cash_cents") + note.get::<i64, _>("balance_reduction_cents");
        let returned: serde_json::Value =
            serde_json::from_str(&note.get::<String, _>("returned_items")).unwrap_or_default();
        let lines = returned.as_array().cloned().unwrap_or_default();
        let sole = items.len() == 1 && lines.len() == 1;
        for line in &lines {
            let quantity = line.get("quantity").and_then(|q| q.as_i64()).unwrap_or(0);
            let product_key = line.get("productKey").and_then(|v| v.as_str());
            let ticket_key = line.get("sourceTicketKey").and_then(|v| v.as_str());
            let name = line.get("name").and_then(|v| v.as_str());
            if let Some(idx) = line_match(&items, product_key, ticket_key, name, sole) {
                items[idx].returned_quantity += quantity;
            }
        }
    }
    let count = notes.len() as i64;

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

    // Credit sales settle through payments; voided/closed are sticky.
    let mut new_status = status.clone();
    if is_credit != 0 && matches!(status.as_str(), "pending" | "partially_paid" | "paid") {
        let paid: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_cents), 0) FROM payments WHERE invoice_key = ? AND deleted_at IS NULL",
        )
        .bind(key)
        .fetch_one(&mut *conn)
        .await?;
        new_status = status_str(InvoiceStatus::from_payment_progress(total_cents, paid));
    }

    let items_changed = serde_json::to_value(&items)? != serde_json::to_value(&old_items)?;
    if items_changed || refunded != old_refunded || count != old_count || new_status != status {
        sqlx::query(
            "UPDATE invoices SET items = ?, refunded_cents = ?, credit_note_count = ?, status = ?, \
             version = version + 1, updated_at = ? WHERE key = ?",
        )
        .bind(serde_json::to_string(&items)?)
        .bind(refunded)
        .bind(count)
        .bind(&new_status)
        .bind(now_iso())
        .bind(key)
        .execute(&mut *conn)
        .await?;
    }
    Ok(customer_key)
}

/// `total_purchases_cents` and `outstanding_balance_cents` from the invoice /
/// payment / credit-note ledger (spec 1.3), never from incremental deltas.
async fn recompute_customer(conn: &mut SqliteConnection, key: &str) -> AppResult<()> {
    let Some(row) = sqlx::query(
        "SELECT outstanding_balance_cents, total_purchases_cents FROM customers WHERE key = ? AND deleted_at IS NULL",
    )
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(());
    };
    let old_outstanding: i64 = row.get("outstanding_balance_cents");
    let old_purchases: i64 = row.get("total_purchases_cents");

    let sales: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total_cents), 0) FROM invoices \
         WHERE customer_key = ? AND status != 'voided' AND deleted_at IS NULL",
    )
    .bind(key)
    .fetch_one(&mut *conn)
    .await?;
    let refunds: i64 = sqlx::query_scalar(
        // A credit note reaches its customer directly (no-receipt returns) or
        // through the invoice it returns against.
        "SELECT COALESCE(SUM(refund_cash_cents), 0) FROM credit_notes \
         WHERE status != 'voided' AND deleted_at IS NULL \
           AND (customer_key = ?1 OR invoice_key IN (SELECT key FROM invoices WHERE customer_key = ?1))",
    )
    .bind(key)
    .fetch_one(&mut *conn)
    .await?;
    let outstanding: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(i.total_cents - COALESCE(\
             (SELECT SUM(p.amount_cents) FROM payments p WHERE p.invoice_key = i.key AND p.deleted_at IS NULL), 0)), 0) \
         FROM invoices i \
         WHERE i.customer_key = ? AND i.is_credit = 1 AND i.status != 'voided' AND i.deleted_at IS NULL",
    )
    .bind(key)
    .fetch_one(&mut *conn)
    .await?;

    let purchases = sales - refunds;
    if purchases != old_purchases || outstanding != old_outstanding {
        sqlx::query(
            "UPDATE customers SET total_purchases_cents = ?, outstanding_balance_cents = ?, \
             version = version + 1, updated_at = ? WHERE key = ?",
        )
        .bind(purchases)
        .bind(outstanding)
        .bind(now_iso())
        .bind(key)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// `stock_quantity` = the sum of the product's stock movements. A negative
/// result is kept (never clamped) and reported once per negative episode.
async fn recompute_product(
    conn: &mut SqliteConnection,
    product_id: &str,
    conflicts: &mut Vec<NewConflict>,
) -> AppResult<()> {
    let Some(row) = sqlx::query("SELECT key, stock_quantity FROM products WHERE id = ? AND deleted_at IS NULL")
        .bind(product_id)
        .fetch_optional(&mut *conn)
        .await?
    else {
        return Ok(());
    };
    let product_key: String = row.get("key");
    let old: i64 = row.get("stock_quantity");

    let ledger: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(quantity_delta), 0) FROM stock_movements WHERE product_id = ? AND deleted_at IS NULL",
    )
    .bind(product_id)
    .fetch_one(&mut *conn)
    .await?;

    if ledger != old {
        sqlx::query("UPDATE products SET stock_quantity = ?, version = version + 1, updated_at = ? WHERE id = ?")
            .bind(ledger)
            .bind(now_iso())
            .bind(product_id)
            .execute(&mut *conn)
            .await?;
    }

    if ledger < 0 {
        let already_open: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sync_conflicts WHERE kind = 'NEGATIVE_STOCK' AND entity_key = ? AND resolved_at IS NULL",
        )
        .bind(&product_key)
        .fetch_one(&mut *conn)
        .await?;
        if already_open == 0 {
            conflicts.push(NewConflict {
                kind: ConflictKind::NegativeStock,
                resource: "products".to_string(),
                entity_key: product_key.clone(),
                detail: serde_json::json!({ "productKey": product_key, "quantity": ledger }),
            });
        }
    }
    Ok(())
}

/// Persists conflicts raised while applying or recomputing.
pub async fn store_conflicts(
    conn: &mut SqliteConnection,
    conflicts: &[NewConflict],
) -> AppResult<()> {
    for conflict in conflicts {
        sqlx::query(
            "INSERT INTO sync_conflicts (key, kind, resource, entity_key, detail, detected_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(generate_id("scf"))
        .bind(conflict.kind.as_str())
        .bind(&conflict.resource)
        .bind(&conflict.entity_key)
        .bind(serde_json::to_string(&conflict.detail)?)
        .bind(now_iso())
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
