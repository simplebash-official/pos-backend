use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::billing::{InvoiceItem, InvoiceStatus};
use crate::domain::sync_v2::{ConflictKind, DerivedScope, NewConflict};
use crate::modules::billing::service::credit_notes::{LineHint, line_match};
use crate::modules::sync::derived_session::{
    SessionLedger, drawer_delta_for_movement, parse_rfc3339_ms,
};
use chrono::Utc;
use sqlx::{Row, SqliteConnection};
use std::collections::BTreeSet;

pub(super) fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

/// Recomputes every derived field touched by `scope` and returns the
/// invariant conflicts found (over-refund, negative stock).
pub async fn recompute(
    conn: &mut SqliteConnection,
    scope: &DerivedScope,
) -> AppResult<Vec<NewConflict>> {
    let mut conflicts = Vec::new();
    let mut customer_keys: BTreeSet<String> = scope.customer_keys.clone();
    let mut drawer_keys: BTreeSet<String> = scope.drawer_keys.clone();

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
    for batch_key in &scope.batch_keys {
        recompute_batch(conn, batch_key, &mut conflicts).await?;
    }
    for session_key in &scope.session_keys {
        if let Some(cashier_id) = recompute_session(conn, session_key).await? {
            drawer_keys.insert("shop".to_string());
            drawer_keys.insert(cashier_id);
        }
    }
    if scope.refresh_drawers {
        let keys: Vec<String> = sqlx::query_scalar("SELECT key FROM drawer_state")
            .fetch_all(&mut *conn)
            .await?;
        drawer_keys.extend(keys);
    }
    for drawer_key in &drawer_keys {
        recompute_drawer(conn, drawer_key).await?;
    }
    Ok(conflicts)
}

fn status_str(status: InvoiceStatus) -> String {
    serde_json::to_value(status)
        .expect("an invoice status serializes")
        .as_str()
        .expect("an invoice status serializes as a string")
        .to_string()
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

    let old_items: Vec<InvoiceItem> = serde_json::from_str(&items_json)?;
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
        refunded +=
            note.get::<i64, _>("refund_cash_cents") + note.get::<i64, _>("balance_reduction_cents");
        let returned: serde_json::Value =
            serde_json::from_str(&note.get::<String, _>("returned_items"))?;
        let lines = returned
            .as_array()
            .cloned()
            .ok_or_else(|| AppError::internal("a credit note's returned_items is not an array"))?;
        let sole = items.len() == 1 && lines.len() == 1;
        for line in &lines {
            let quantity = line
                .get("quantity")
                .and_then(|q| q.as_i64())
                .ok_or_else(|| AppError::internal("a returned line has no quantity"))?;
            let product_key = line.get("productKey").and_then(|v| v.as_str());
            let ticket_key = line.get("sourceTicketKey").and_then(|v| v.as_str());
            let name = line.get("name").and_then(|v| v.as_str());
            let batch_keys: Vec<&str> = line
                .get("batchAllocations")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|a| a.get("batchKey").and_then(|k| k.as_str()))
                .collect();
            let hint = LineHint {
                unit_price_cents: None,
                batch_keys: &batch_keys,
            };
            if let Some(idx) = line_match(&items, product_key, ticket_key, name, hint, sole) {
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

/// `remaining_quantity` = the sum of the movements carrying the batch's key.
/// Only the derived column is written (no `version`/`updated_at` bump), so a
/// recompute never competes last-writer-wins with a real price edit. A
/// negative result (two offline sales of the batch's last unit) is kept and
/// reported once per negative episode, like a product's stock.
async fn recompute_batch(
    conn: &mut SqliteConnection,
    batch_key: &str,
    conflicts: &mut Vec<NewConflict>,
) -> AppResult<()> {
    let Some(old) = sqlx::query_scalar::<_, i64>(
        "SELECT remaining_quantity FROM stock_batches WHERE key = ? AND deleted_at IS NULL",
    )
    .bind(batch_key)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(());
    };
    let ledger: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(quantity_delta), 0) FROM stock_movements WHERE batch_key = ? AND deleted_at IS NULL",
    )
    .bind(batch_key)
    .fetch_one(&mut *conn)
    .await?;
    if ledger != old {
        sqlx::query("UPDATE stock_batches SET remaining_quantity = ? WHERE key = ?")
            .bind(ledger)
            .bind(batch_key)
            .execute(&mut *conn)
            .await?;
    }
    if ledger < 0 {
        let already_open: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sync_conflicts WHERE kind = 'NEGATIVE_STOCK' AND entity_key = ? AND resolved_at IS NULL",
        )
        .bind(batch_key)
        .fetch_one(&mut *conn)
        .await?;
        if already_open == 0 {
            conflicts.push(NewConflict {
                kind: ConflictKind::NegativeStock,
                resource: "stockBatches".to_string(),
                entity_key: batch_key.to_string(),
                detail: serde_json::json!({ "batchKey": batch_key, "quantity": ledger }),
            });
        }
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
    let Some(row) =
        sqlx::query("SELECT key, stock_quantity FROM products WHERE id = ? AND deleted_at IS NULL")
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

/// Recomputes `register_sessions` derived totals, `has_late_movements`, and
/// (when `closing`/`closed`) `expected_cash_cents`/`cash_variance_cents` from
/// the `payments`, `invoices`, and `cash_movements` ledgers.
async fn recompute_session(conn: &mut SqliteConnection, key: &str) -> AppResult<Option<String>> {
    let Some(row) = sqlx::query(
        "SELECT cashier_id, status, opening_float_cents, counted_cash_cents, \
                total_cash_sales_cents, total_card_sales_cents, total_online_sales_cents, \
                total_credit_sales_cents, total_cash_refunds_cents, total_pay_in_cents, \
                total_pay_out_cents, total_owner_withdrawals_cents, total_cash_drops_cents, \
                has_late_movements, expected_cash_cents, cash_variance_cents \
         FROM register_sessions WHERE key = ? AND deleted_at IS NULL",
    )
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };

    let cashier_id: String = row.get("cashier_id");
    let status: String = row.get("status");
    let opening_float: i64 = row.get("opening_float_cents");
    let counted_cash: Option<i64> = row.get("counted_cash_cents");

    let mut ledger = SessionLedger::default();

    let payment_rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT amount_cents, payment_method, notes FROM payments \
         WHERE session_key = ? AND deleted_at IS NULL",
    )
    .bind(key)
    .fetch_all(&mut *conn)
    .await?;
    for (amount, method, notes) in payment_rows {
        ledger.record_payment(amount, &method, notes.as_deref());
    }

    let credit_rows: Vec<(i64, Option<i64>)> = sqlx::query_as(
        "SELECT total_cents, amount_received_cents FROM invoices \
         WHERE session_key = ? AND is_credit != 0 AND deleted_at IS NULL",
    )
    .bind(key)
    .fetch_all(&mut *conn)
    .await?;
    for (total, received) in credit_rows {
        ledger.record_credit_invoice(total, received);
    }

    let movement_rows: Vec<(String, i64, Option<String>, i64)> = sqlx::query_as(
        "SELECT movement_type, amount_cents, reverses_key, recorded_late FROM cash_movements \
         WHERE session_key = ? AND deleted_at IS NULL",
    )
    .bind(key)
    .fetch_all(&mut *conn)
    .await?;
    for (movement_type, amount, reverses_key, recorded_late) in movement_rows {
        ledger.record_movement(
            &movement_type,
            amount,
            reverses_key.is_some(),
            recorded_late != 0,
        );
    }

    let (expected_opt, variance_opt) = ledger.closing_cash(&status, opening_float, counted_cash);

    let old_cash_sales: i64 = row.get("total_cash_sales_cents");
    let old_card_sales: i64 = row.get("total_card_sales_cents");
    let old_online_sales: i64 = row.get("total_online_sales_cents");
    let old_credit_sales: i64 = row.get("total_credit_sales_cents");
    let old_cash_refunds: i64 = row.get("total_cash_refunds_cents");
    let old_pay_in: i64 = row.get("total_pay_in_cents");
    let old_pay_out: i64 = row.get("total_pay_out_cents");
    let old_withdrawals: i64 = row.get("total_owner_withdrawals_cents");
    let old_drops: i64 = row.get("total_cash_drops_cents");
    let old_late: i64 = row.get("has_late_movements");
    let old_expected: Option<i64> = row.get("expected_cash_cents");
    let old_variance: Option<i64> = row.get("cash_variance_cents");

    let unchanged = old_cash_sales == ledger.total_cash_sales_cents
        && old_card_sales == ledger.total_card_sales_cents
        && old_online_sales == ledger.total_online_sales_cents
        && old_credit_sales == ledger.total_credit_sales_cents
        && old_cash_refunds == ledger.total_cash_refunds_cents
        && old_pay_in == ledger.total_pay_in_cents
        && old_pay_out == ledger.total_pay_out_cents
        && old_withdrawals == ledger.total_owner_withdrawals_cents
        && old_drops == ledger.total_cash_drops_cents
        && (old_late != 0) == ledger.has_late_movements
        && old_expected == expected_opt
        && old_variance == variance_opt;

    if !unchanged {
        sqlx::query(
            "UPDATE register_sessions SET \
             total_cash_sales_cents = ?, total_card_sales_cents = ?, total_online_sales_cents = ?, \
             total_credit_sales_cents = ?, total_cash_refunds_cents = ?, total_pay_in_cents = ?, \
             total_pay_out_cents = ?, total_owner_withdrawals_cents = ?, total_cash_drops_cents = ?, \
             has_late_movements = ?, expected_cash_cents = ?, cash_variance_cents = ?, \
             version = version + 1, updated_at = ? \
             WHERE key = ?",
        )
        .bind(ledger.total_cash_sales_cents)
        .bind(ledger.total_card_sales_cents)
        .bind(ledger.total_online_sales_cents)
        .bind(ledger.total_credit_sales_cents)
        .bind(ledger.total_cash_refunds_cents)
        .bind(ledger.total_pay_in_cents)
        .bind(ledger.total_pay_out_cents)
        .bind(ledger.total_owner_withdrawals_cents)
        .bind(ledger.total_cash_drops_cents)
        .bind(i64::from(ledger.has_late_movements))
        .bind(expected_opt)
        .bind(variance_opt)
        .bind(now_iso())
        .bind(key)
        .execute(&mut *conn)
        .await?;
    }

    Ok(Some(cashier_id))
}

/// Recomputes `drawer_state.current_balance_cents` from the owning active shift's
/// expected cash, or when between shifts, from `last_closing_float_cents` plus
/// unlinked cash movements recorded after the latest closed shift.
async fn recompute_drawer(conn: &mut SqliteConnection, drawer_key: &str) -> AppResult<()> {
    let Some(drawer_row) = sqlx::query(
        "SELECT current_balance_cents, last_session_key, last_closing_float_cents \
         FROM drawer_state WHERE key = ?",
    )
    .bind(drawer_key)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(());
    };
    let old_balance: i64 = drawer_row.get("current_balance_cents");
    let last_session_key: Option<String> = drawer_row.get("last_session_key");
    let last_closing_float: i64 = drawer_row.get("last_closing_float_cents");

    let shared_shop_opt: Option<i64> = sqlx::query_scalar(
        "SELECT allow_shared_user_sessions FROM shop_profiles WHERE key = 'main' AND deleted_at IS NULL LIMIT 1",
    )
    .fetch_optional(&mut *conn)
    .await?;
    let shared_bool_opt = shared_shop_opt.map(|v| v != 0);

    let active_mode_matches = if drawer_key == "shop" {
        shared_bool_opt != Some(false)
    } else {
        shared_bool_opt != Some(true)
    };

    let target_balance = if active_mode_matches {
        let active_rows = if drawer_key == "shop" {
            sqlx::query(
                "SELECT key, opened_at, opening_float_cents, total_cash_sales_cents, \
                        total_pay_in_cents, total_pay_out_cents, total_cash_refunds_cents, \
                        total_owner_withdrawals_cents, total_cash_drops_cents \
                 FROM register_sessions \
                 WHERE status IN ('open', 'closing', 'reopened') AND deleted_at IS NULL",
            )
            .fetch_all(&mut *conn)
            .await?
        } else {
            sqlx::query(
                "SELECT key, opened_at, opening_float_cents, total_cash_sales_cents, \
                        total_pay_in_cents, total_pay_out_cents, total_cash_refunds_cents, \
                        total_owner_withdrawals_cents, total_cash_drops_cents \
                 FROM register_sessions \
                 WHERE cashier_id = ? AND status IN ('open', 'closing', 'reopened') AND deleted_at IS NULL",
            )
            .bind(drawer_key)
            .fetch_all(&mut *conn)
            .await?
        };
        let mut active_candidates: Vec<(i64, String, i64)> = active_rows
            .into_iter()
            .map(|r| {
                let opened: String = r.get("opened_at");
                let k: String = r.get("key");
                let expected: i64 = r.get::<i64, _>("opening_float_cents")
                    + r.get::<i64, _>("total_cash_sales_cents")
                    + r.get::<i64, _>("total_pay_in_cents")
                    - r.get::<i64, _>("total_pay_out_cents")
                    - r.get::<i64, _>("total_cash_refunds_cents")
                    - r.get::<i64, _>("total_owner_withdrawals_cents")
                    - r.get::<i64, _>("total_cash_drops_cents");
                (parse_rfc3339_ms(&opened), k, expected)
            })
            .collect();
        active_candidates.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        if let Some((_, _, expected)) = active_candidates.into_iter().next() {
            expected
        } else {
            let cutoff_ms =
                latest_closed_at_ms_sqlite(conn, last_session_key.as_deref(), drawer_key).await?;
            let between_delta = between_shift_delta_sqlite(conn, drawer_key, cutoff_ms).await?;
            last_closing_float + between_delta
        }
    } else {
        last_closing_float
    };

    if target_balance != old_balance {
        sqlx::query(
            "UPDATE drawer_state SET current_balance_cents = ?, version = version + 1, updated_at = ? \
             WHERE key = ?",
        )
        .bind(target_balance)
        .bind(now_iso())
        .bind(drawer_key)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn latest_closed_at_ms_sqlite(
    conn: &mut SqliteConnection,
    last_session_key: Option<&str>,
    drawer_key: &str,
) -> AppResult<Option<i64>> {
    let mut best: Option<i64> = None;
    if let Some(last_key) = last_session_key {
        let closed_at: Option<Option<String>> = sqlx::query_scalar(
            "SELECT closed_at FROM register_sessions \
             WHERE key = ? AND status = 'closed' AND deleted_at IS NULL",
        )
        .bind(last_key)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(Some(ts)) = closed_at {
            best = Some(parse_rfc3339_ms(&ts));
        }
    }
    let closed_timestamps: Vec<Option<String>> = if drawer_key == "shop" {
        sqlx::query_scalar(
            "SELECT closed_at FROM register_sessions WHERE status = 'closed' AND deleted_at IS NULL",
        )
        .fetch_all(&mut *conn)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT closed_at FROM register_sessions \
             WHERE cashier_id = ? AND status = 'closed' AND deleted_at IS NULL",
        )
        .bind(drawer_key)
        .fetch_all(&mut *conn)
        .await?
    };
    for ts in closed_timestamps.into_iter().flatten() {
        let ms = parse_rfc3339_ms(&ts);
        best = Some(best.map_or(ms, |cur| cur.max(ms)));
    }
    Ok(best)
}

async fn between_shift_delta_sqlite(
    conn: &mut SqliteConnection,
    drawer_key: &str,
    cutoff_ms: Option<i64>,
) -> AppResult<i64> {
    let rows: Vec<(String, i64, Option<String>, String)> = if drawer_key == "shop" {
        sqlx::query_as(
            "SELECT movement_type, amount_cents, reverses_key, created_at FROM cash_movements \
             WHERE session_key IS NULL AND deleted_at IS NULL",
        )
        .fetch_all(&mut *conn)
        .await?
    } else {
        sqlx::query_as(
            "SELECT movement_type, amount_cents, reverses_key, created_at FROM cash_movements \
             WHERE session_key IS NULL AND recorded_by_user_id = ? AND deleted_at IS NULL",
        )
        .bind(drawer_key)
        .fetch_all(&mut *conn)
        .await?
    };
    let mut delta = 0i64;
    for (movement_type, amount, reverses_key, created_at) in rows {
        if let Some(cutoff) = cutoff_ms
            && parse_rfc3339_ms(&created_at) <= cutoff
        {
            continue;
        }
        delta += drawer_delta_for_movement(&movement_type, amount, reverses_key.is_some());
    }
    Ok(delta)
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
