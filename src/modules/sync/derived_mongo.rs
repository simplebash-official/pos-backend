//! Derived-field recompute for the cloud (MongoDB) engine: stock, customer
//! balances and an invoice's refund totals / per-line returned quantity /
//! credit-sale status are recomputed from the ledger rows after every applied
//! batch, exactly as `derived_sqlite` does on a device (formulas: sync v2 spec
//! 1.3), so both engines converge on the ledger truth.
//!
//! One deliberate difference from the device engine: only the derived columns
//! are written; `updated_at`, `version` and `updated_by_device` are left alone.
//! The cloud reads `updated_at` for last-writer-wins, so bumping it here would
//! let a recompute of stale ordinary columns beat a concurrent real edit.

use std::collections::BTreeSet;

use futures_util::TryStreamExt;
use mongodb::bson;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, Document, doc};

use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::domain::billing::{InvoiceItem, InvoiceStatus};

use crate::domain::sync_v2::{ConflictKind, DerivedScope, NewConflict};

use crate::modules::billing::service::credit_notes::{LineHint, line_match};

use crate::modules::sync::apply_mongo::{num_i64, required_i64, required_str};

use crate::modules::sync::cloud_store::coll;
use crate::modules::sync::derived_session::{SessionLedger, drawer_delta_for_movement};

/// Recomputes every derived field touched by `scope`; returns the invariant
/// conflicts found (over-refund, negative stock).
pub(crate) async fn recompute_derived(
    db: &Db,
    scope: &DerivedScope,
) -> AppResult<Vec<NewConflict>> {
    let mut conflicts = Vec::new();
    let mut customer_keys: BTreeSet<String> = scope.customer_keys.clone();
    let mut drawer_keys: BTreeSet<String> = scope.drawer_keys.clone();

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
    for batch_key in &scope.batch_keys {
        recompute_batch(db, batch_key, &mut conflicts).await?;
    }
    for session_key in &scope.session_keys {
        if let Some(cashier_id) = recompute_session(db, session_key).await? {
            drawer_keys.insert("shop".to_string());
            drawer_keys.insert(cashier_id);
        }
    }
    if scope.refresh_drawers {
        let mut cursor = coll(db, "drawer_state")?.find(doc! {}).await?;
        while let Some(doc) = cursor.try_next().await? {
            if let Ok(key) = doc.get_str("key") {
                drawer_keys.insert(key.to_string());
            }
        }
    }
    for drawer_key in &drawer_keys {
        recompute_drawer(db, drawer_key).await?;
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
        Some(row) => required_i64(&row, "total")?,
        None => 0,
    })
}

fn status_str(status: InvoiceStatus) -> String {
    serde_json::to_value(status)
        .expect("an invoice status serializes")
        .as_str()
        .expect("an invoice status serializes as a string")
        .to_string()
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
    let total_cents = required_i64(&invoice, "total_cents")?;
    let is_credit = invoice
        .get_bool("is_credit")
        .map_err(|_| AppError::internal("stored invoice has no `is_credit`"))?;
    let status = required_str(&invoice, "status")?.to_string();
    // A derived column is absent until its first recompute, so "no old value" is expected here.
    let old_refunded = num_i64(&invoice, "refunded_cents");
    let old_count = num_i64(&invoice, "credit_note_count");
    let customer_key = invoice.get_str("customer_key").ok().map(str::to_string);

    let old_items: Vec<InvoiceItem> = bson::deserialize_from_bson(
        invoice
            .get("items")
            .cloned()
            .ok_or_else(|| AppError::internal("stored invoice has no items"))?,
    )
    .map_err(|e| AppError::internal(format!("invoice items are unreadable: {e}")))?;
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
        refunded += required_i64(&note, "refund_cash_cents")?
            + required_i64(&note, "balance_reduction_cents")?;
        let lines: Vec<Document> = note
            .get_array("returned_items")
            .map_err(|err| AppError::internal(format!("credit note has no returned_items: {err}")))?
            .iter()
            .map(|line| {
                line.as_document().cloned().ok_or_else(|| {
                    AppError::internal("a credit note's returned line is not a document")
                })
            })
            .collect::<AppResult<_>>()?;
        let sole = items.len() == 1 && lines.len() == 1;
        for line in &lines {
            let quantity = required_i64(line, "quantity")?;
            let batch_keys: Vec<&str> = line
                .get_array("batchAllocations")
                .into_iter()
                .flatten()
                .filter_map(|a| a.as_document()?.get_str("batchKey").ok())
                .collect();
            if let Some(idx) = line_match(
                &items,
                line.get_str("productKey").ok(),
                line.get_str("sourceTicketKey").ok(),
                line.get_str("name").ok(),
                LineHint {
                    unit_price_cents: None,
                    batch_keys: &batch_keys,
                },
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
    if items_changed
        || Some(refunded) != old_refunded
        || Some(count) != old_count
        || new_status != status
    {
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
    let old_outstanding = num_i64(&customer, "outstanding_balance_cents");
    let old_purchases = num_i64(&customer, "total_purchases_cents");

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
            doc! { "invoice_key": required_str(&invoice, "key")? },
            "amount_cents",
        )
        .await?;
        outstanding += required_i64(&invoice, "total_cents")? - paid;
    }

    let purchases = sales - refunds;
    if Some(outstanding) != old_outstanding || Some(purchases) != old_purchases {
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

/// `remaining_quantity` = SUM of the movements carrying the batch's key. A
/// batch that becomes negative raises `NEGATIVE_STOCK` once, at the transition.
async fn recompute_batch(
    db: &Db,
    batch_key: &str,
    conflicts: &mut Vec<NewConflict>,
) -> AppResult<()> {
    let batches = coll(db, "stock_batches")?.preserve_origin();
    let Some(batch) = batches.find_one(doc! { "key": batch_key }).await? else {
        return Ok(());
    };
    let old = num_i64(&batch, "remaining_quantity");
    let total = sum_field(
        db,
        "stock_movements",
        doc! { "batch_key": batch_key },
        "quantity_delta",
    )
    .await?;
    if Some(total) == old {
        return Ok(());
    }
    batches
        .update_one(
            doc! { "key": batch_key },
            doc! { "$set": { "remaining_quantity": total } },
        )
        .await?;
    if total < 0 && old.is_none_or(|old| old >= 0) {
        conflicts.push(NewConflict {
            kind: ConflictKind::NegativeStock,
            resource: "stockBatches".to_string(),
            entity_key: batch_key.to_string(),
            detail: serde_json::json!({ "batchKey": batch_key, "quantity": total }),
        });
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
    let old = num_i64(&product, "stock_quantity");
    let total = sum_field(
        db,
        "stock_movements",
        doc! { "product_id": oid },
        "quantity_delta",
    )
    .await?;
    if Some(total) == old {
        return Ok(());
    }
    products
        .update_one(
            doc! { "_id": oid },
            doc! { "$set": { "stock_quantity": total } },
        )
        .await?;
    if total < 0 && old.is_none_or(|old| old >= 0) {
        conflicts.push(NewConflict {
            kind: ConflictKind::NegativeStock,
            resource: "products".to_string(),
            entity_key: required_str(&product, "key")?.to_string(),
            detail: serde_json::json!({
                "productKey": required_str(&product, "key")?,
                "quantity": total,
            }),
        });
    }
    Ok(())
}

/// Recomputes `register_sessions` derived totals, `has_late_movements`, and
/// (when `closing`/`closed`) `expected_cash_cents`/`cash_variance_cents` from
/// the `payments`, `invoices`, and `cash_movements` ledgers.
async fn recompute_session(db: &Db, key: &str) -> AppResult<Option<String>> {
    let sessions = coll(db, "register_sessions")?.preserve_origin();
    let Some(doc) = sessions
        .find_one(doc! { "key": key, "deleted_at": Bson::Null })
        .await?
    else {
        return Ok(None);
    };
    let cashier_id = doc.get_str("cashier_id").ok().map(str::to_string);
    let status = doc.get_str("status").unwrap_or("open");
    let opening_float = num_i64(&doc, "opening_float_cents").unwrap_or(0);
    let counted_cash = num_i64(&doc, "counted_cash_cents");

    let mut ledger = SessionLedger::default();

    let mut payments = coll(db, "payments")?
        .find(doc! { "session_key": key, "deleted_at": Bson::Null })
        .await?;
    while let Some(pay) = payments.try_next().await? {
        let amount = num_i64(&pay, "amount_cents").unwrap_or(0);
        let method = pay.get_str("payment_method").unwrap_or("");
        let notes = pay.get_str("notes").ok();
        ledger.record_payment(amount, method, notes);
    }

    let mut credit_invoices = coll(db, "invoices")?
        .find(doc! { "session_key": key, "is_credit": true, "deleted_at": Bson::Null })
        .await?;
    while let Some(inv) = credit_invoices.try_next().await? {
        let total = num_i64(&inv, "total_cents").unwrap_or(0);
        let received = num_i64(&inv, "amount_received_cents");
        ledger.record_credit_invoice(total, received);
    }

    let mut movements = coll(db, "cash_movements")?
        .find(doc! { "session_key": key, "deleted_at": Bson::Null })
        .await?;
    while let Some(mov) = movements.try_next().await? {
        let movement_type = mov.get_str("movement_type").unwrap_or("");
        let amount = num_i64(&mov, "amount_cents").unwrap_or(0);
        let reversed = mov.get_str("reverses_key").ok().is_some();
        let recorded_late = mov.get_bool("recorded_late").unwrap_or(false);
        ledger.record_movement(movement_type, amount, reversed, recorded_late);
    }

    let (expected_opt, variance_opt) = ledger.closing_cash(status, opening_float, counted_cash);

    let unchanged = num_i64(&doc, "total_cash_sales_cents") == Some(ledger.total_cash_sales_cents)
        && num_i64(&doc, "total_card_sales_cents") == Some(ledger.total_card_sales_cents)
        && num_i64(&doc, "total_online_sales_cents") == Some(ledger.total_online_sales_cents)
        && num_i64(&doc, "total_credit_sales_cents") == Some(ledger.total_credit_sales_cents)
        && num_i64(&doc, "total_cash_refunds_cents") == Some(ledger.total_cash_refunds_cents)
        && num_i64(&doc, "total_pay_in_cents") == Some(ledger.total_pay_in_cents)
        && num_i64(&doc, "total_pay_out_cents") == Some(ledger.total_pay_out_cents)
        && num_i64(&doc, "total_owner_withdrawals_cents")
            == Some(ledger.total_owner_withdrawals_cents)
        && num_i64(&doc, "total_cash_drops_cents") == Some(ledger.total_cash_drops_cents)
        && doc.get_bool("has_late_movements").ok() == Some(ledger.has_late_movements)
        && num_i64(&doc, "expected_cash_cents") == expected_opt
        && num_i64(&doc, "cash_variance_cents") == variance_opt;

    if unchanged {
        return Ok(cashier_id);
    }

    let mut set_doc = doc! {
        "total_cash_sales_cents": ledger.total_cash_sales_cents,
        "total_card_sales_cents": ledger.total_card_sales_cents,
        "total_online_sales_cents": ledger.total_online_sales_cents,
        "total_credit_sales_cents": ledger.total_credit_sales_cents,
        "total_cash_refunds_cents": ledger.total_cash_refunds_cents,
        "total_pay_in_cents": ledger.total_pay_in_cents,
        "total_pay_out_cents": ledger.total_pay_out_cents,
        "total_owner_withdrawals_cents": ledger.total_owner_withdrawals_cents,
        "total_cash_drops_cents": ledger.total_cash_drops_cents,
        "has_late_movements": ledger.has_late_movements,
    };
    let mut unset_doc = Document::new();
    match expected_opt {
        Some(v) => {
            set_doc.insert("expected_cash_cents", v);
        }
        None if doc.contains_key("expected_cash_cents") => {
            unset_doc.insert("expected_cash_cents", "");
        }
        None => {}
    }
    match variance_opt {
        Some(v) => {
            set_doc.insert("cash_variance_cents", v);
        }
        None if doc.contains_key("cash_variance_cents") => {
            unset_doc.insert("cash_variance_cents", "");
        }
        None => {}
    }

    let mut update = doc! { "$set": set_doc };
    if !unset_doc.is_empty() {
        update.insert("$unset", unset_doc);
    }
    sessions.update_one(doc! { "key": key }, update).await?;
    Ok(cashier_id)
}

/// Computes `calculate_expected_cash` directly from a stored `register_sessions` document.
fn expected_cash_from_session_doc(doc: &Document) -> i64 {
    num_i64(doc, "opening_float_cents").unwrap_or(0)
        + num_i64(doc, "total_cash_sales_cents").unwrap_or(0)
        + num_i64(doc, "total_pay_in_cents").unwrap_or(0)
        - num_i64(doc, "total_pay_out_cents").unwrap_or(0)
        - num_i64(doc, "total_cash_refunds_cents").unwrap_or(0)
        - num_i64(doc, "total_owner_withdrawals_cents").unwrap_or(0)
        - num_i64(doc, "total_cash_drops_cents").unwrap_or(0)
}

/// Recomputes `drawer_state.current_balance_cents` from the owning active shift's
/// expected cash, or when between shifts, from `last_closing_float_cents` plus
/// unlinked cash movements recorded after the latest closed shift.
async fn recompute_drawer(db: &Db, drawer_key: &str) -> AppResult<()> {
    let drawers = coll(db, "drawer_state")?.preserve_origin();
    let Some(drawer) = drawers.find_one(doc! { "key": drawer_key }).await? else {
        return Ok(());
    };
    let shared_shop_opt = coll(db, "shop_profiles")?
        .find_one(doc! { "key": "main", "deleted_at": Bson::Null })
        .await?
        .and_then(|d| d.get_bool("allow_shared_user_sessions").ok());

    let active_mode_matches = if drawer_key == "shop" {
        shared_shop_opt != Some(false)
    } else {
        shared_shop_opt != Some(true)
    };

    let target_balance = if active_mode_matches {
        let mut active_filter = doc! {
            "status": { "$in": ["open", "closing", "reopened"] },
            "deleted_at": Bson::Null,
        };
        if drawer_key != "shop" {
            active_filter.insert("cashier_id", drawer_key);
        }
        let mut cursor = coll(db, "register_sessions")?.find(active_filter).await?;
        let mut active_candidates: Vec<(i64, String, Document)> = Vec::new();
        while let Some(sess) = cursor.try_next().await? {
            let opened_ms = sess
                .get_datetime("opened_at")
                .map(|d| d.timestamp_millis())
                .unwrap_or(0);
            let k = sess.get_str("key").unwrap_or("").to_string();
            active_candidates.push((opened_ms, k, sess));
        }
        active_candidates.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        if let Some((_, _, active)) = active_candidates.into_iter().next() {
            expected_cash_from_session_doc(&active)
        } else {
            let base = num_i64(&drawer, "last_closing_float_cents").unwrap_or(0);
            let cutoff_ms = latest_closed_at_ms_mongo(db, &drawer, drawer_key).await?;
            let between_delta = between_shift_delta_mongo(db, drawer_key, cutoff_ms).await?;
            base + between_delta
        }
    } else {
        num_i64(&drawer, "last_closing_float_cents").unwrap_or(0)
    };

    if Some(target_balance) == num_i64(&drawer, "current_balance_cents") {
        return Ok(());
    }
    drawers
        .update_one(
            doc! { "key": drawer_key },
            doc! { "$set": { "current_balance_cents": target_balance } },
        )
        .await?;
    Ok(())
}

async fn latest_closed_at_ms_mongo(
    db: &Db,
    drawer: &Document,
    drawer_key: &str,
) -> AppResult<Option<i64>> {
    let mut best: Option<i64> = None;
    if let Ok(last_key) = drawer.get_str("last_session_key")
        && let Some(sess) = coll(db, "register_sessions")?
            .find_one(doc! { "key": last_key, "status": "closed", "deleted_at": Bson::Null })
            .await?
        && let Ok(closed_at) = sess.get_datetime("closed_at")
    {
        best = Some(closed_at.timestamp_millis());
    }
    let mut closed_filter = doc! { "status": "closed", "deleted_at": Bson::Null };
    if drawer_key != "shop" {
        closed_filter.insert("cashier_id", drawer_key);
    }
    let mut cursor = coll(db, "register_sessions")?.find(closed_filter).await?;
    while let Some(sess) = cursor.try_next().await? {
        if let Ok(closed_at) = sess.get_datetime("closed_at") {
            let ms = closed_at.timestamp_millis();
            best = Some(best.map_or(ms, |cur| cur.max(ms)));
        }
    }
    Ok(best)
}

async fn between_shift_delta_mongo(
    db: &Db,
    drawer_key: &str,
    cutoff_ms: Option<i64>,
) -> AppResult<i64> {
    let mut filter = doc! { "session_key": Bson::Null, "deleted_at": Bson::Null };
    if drawer_key != "shop" {
        filter.insert("recorded_by_user_id", drawer_key);
    }
    let mut cursor = coll(db, "cash_movements")?.find(filter).await?;
    let mut delta = 0i64;
    while let Some(mov) = cursor.try_next().await? {
        if let Some(cutoff) = cutoff_ms {
            let created_ms = mov
                .get_datetime("created_at")
                .map(|d| d.timestamp_millis())
                .unwrap_or(0);
            if created_ms <= cutoff {
                continue;
            }
        }
        let movement_type = mov.get_str("movement_type").unwrap_or("");
        let amount = num_i64(&mov, "amount_cents").unwrap_or(0);
        let reversed = mov.get_str("reverses_key").ok().is_some();
        delta += drawer_delta_for_movement(movement_type, amount, reversed);
    }
    Ok(delta)
}
