// Mongo and SQLite access for the `invoices`, `payments`, and `credit_notes`
// collections. Same never-interpret-a-miss-as-an-error convention as every
// other repository in this codebase — `service` decides what a missing row
// means.

pub(crate) mod credit_notes;
pub(crate) mod invoice;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, now_utc_iso, to_bson_datetime},
    },
    core::error::AppResult,
    domain::billing::{
        CreditNoteStatus, InvoiceItem, InvoiceStatus, RefundBreakdownLeg, SplitPayment,
    },
    modules::billing::model::{
        CreditNoteDocument, CreditNoteItemDocument, InvoiceDocument, PaymentDocument,
    },
};

fn invoices(db: &Database) -> Collection<InvoiceDocument> {
    db.collection("invoices")
}

fn payments(db: &Database) -> Collection<PaymentDocument> {
    db.collection("payments")
}

pub(crate) fn invoice_from_sqlite_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<InvoiceDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let items_str: String = row.get("items");
    let items: Vec<InvoiceItem> = serde_json::from_str(&items_str).unwrap_or_default();
    let split_payments_str: Option<String> = row.get("split_payments");
    let split_payments: Option<Vec<SplitPayment>> =
        split_payments_str.and_then(|s| serde_json::from_str(&s).ok());
    let is_credit_int: i64 = row.get("is_credit");
    let status_str: String = row.get("status");
    let status: InvoiceStatus = serde_json::from_value(serde_json::Value::String(status_str))
        .unwrap_or(InvoiceStatus::Paid);
    let shop_profile_str: String = row.get("shop_profile_snapshot");
    let shop_profile_snapshot: serde_json::Value =
        serde_json::from_str(&shop_profile_str).unwrap_or(serde_json::Value::Null);

    let voided_at_str: Option<String> = row.get("voided_at");
    let closed_at_str: Option<String> = row.get("closed_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(InvoiceDocument {
        id,
        key: row.get("key"),
        invoice_number: row.get("invoice_number"),
        customer_key: row.get("customer_key"),
        customer_name_snapshot: row.get("customer_name_snapshot"),
        customer_phone_snapshot: row.get("customer_phone_snapshot"),
        customer_address_snapshot: row.get("customer_address_snapshot"),
        cashier_id: row.get("cashier_id"),
        cashier_name_snapshot: row.get("cashier_name_snapshot"),
        items,
        subtotal_cents: row.get("subtotal_cents"),
        discount_type: row.get("discount_type"),
        discount_value: row.get("discount_value"),
        discount_cents: row.get("discount_cents"),
        total_cents: row.get("total_cents"),
        payment_method: row.get("payment_method"),
        split_payments,
        is_credit: is_credit_int != 0,
        amount_received_cents: row.get("amount_received_cents"),
        change_due_cents: row.get("change_due_cents"),
        due_date: row.get("due_date"),
        card_last4: row.get("card_last4"),
        card_ref: row.get("card_ref"),
        online_ref: row.get("online_ref"),
        online_note: row.get("online_note"),
        status,
        notes: row.get("notes"),
        shop_profile_snapshot,
        warranty_terms_snapshot: row.get("warranty_terms_snapshot"),
        document_selection: row.get("document_selection"),
        voided_at: voided_at_str.map(|s| to_bson_datetime(&s)),
        voided_by: row.get("voided_by"),
        voided_reason: row.get("voided_reason"),
        closed_at: closed_at_str.map(|s| to_bson_datetime(&s)),
        closed_by: row.get("closed_by"),
        refunded_cents: row.get("refunded_cents"),
        credit_note_count: row.get("credit_note_count"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}

pub(crate) fn payment_from_sqlite_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<PaymentDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let recorded_at_str: String = row.get("recorded_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(PaymentDocument {
        id,
        key: row.get("key"),
        invoice_key: row.get("invoice_key"),
        amount_cents: row.get("amount_cents"),
        payment_method: row.get("payment_method"),
        notes: row.get("notes"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        recorded_at: to_bson_datetime(&recorded_at_str),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}

pub(crate) fn credit_note_from_sqlite_row(
    row: &sqlx::sqlite::SqliteRow,
) -> AppResult<CreditNoteDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let no_receipt_int: i64 = row.get("no_receipt");
    let returned_items_str: String = row.get("returned_items");
    let returned_items: Vec<CreditNoteItemDocument> =
        serde_json::from_str(&returned_items_str).unwrap_or_default();
    let exchange_items_str: String = row.get("exchange_items");
    let exchange_items: Vec<InvoiceItem> =
        serde_json::from_str(&exchange_items_str).unwrap_or_default();
    let refund_breakdown_str: String = row.get("refund_breakdown");
    let refund_breakdown: Vec<RefundBreakdownLeg> =
        serde_json::from_str(&refund_breakdown_str).unwrap_or_default();
    let refund_payment_keys_str: String = row.get("refund_payment_keys");
    let refund_payment_keys: Vec<String> =
        serde_json::from_str(&refund_payment_keys_str).unwrap_or_default();
    let status_str: String = row.get("status");
    let status: CreditNoteStatus = serde_json::from_value(serde_json::Value::String(status_str))
        .unwrap_or(CreditNoteStatus::Resolved);
    let is_manager_override_int: i64 = row.get("is_manager_override");

    let voided_at_str: Option<String> = row.get("voided_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(CreditNoteDocument {
        id,
        key: row.get("key"),
        credit_note_number: row.get("credit_note_number"),
        invoice_key: row.get("invoice_key"),
        invoice_number: row.get("invoice_number"),
        no_receipt: no_receipt_int != 0,
        customer_key: row.get("customer_key"),
        customer_name_snapshot: row.get("customer_name_snapshot"),
        cashier_id: row.get("cashier_id"),
        cashier_name_snapshot: row.get("cashier_name_snapshot"),
        returned_items,
        exchange_items,
        exchange_reference: row.get("exchange_reference"),
        return_subtotal_cents: row.get("return_subtotal_cents"),
        exchange_subtotal_cents: row.get("exchange_subtotal_cents"),
        net_refund_cents: row.get("net_refund_cents"),
        refund_cash_cents: row.get("refund_cash_cents"),
        balance_reduction_cents: row.get("balance_reduction_cents"),
        refund_breakdown,
        refund_payment_keys,
        status,
        is_manager_override: is_manager_override_int != 0,
        override_approved_by: row.get("override_approved_by"),
        override_reason: row.get("override_reason"),
        notes: row.get("notes"),
        voided_at: voided_at_str.map(|s| to_bson_datetime(&s)),
        voided_by: row.get("voided_by"),
        voided_reason: row.get("voided_reason"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}

fn build_invoice_where_clause(filter: &Document) -> (String, Vec<String>) {
    let mut conditions = vec!["deleted_at IS NULL".to_string()];
    let mut bindings = Vec::new();

    fn process_clause(doc: &Document, conditions: &mut Vec<String>, bindings: &mut Vec<String>) {
        if let Ok(key) = doc.get_str("key") {
            conditions.push("key = ?".to_string());
            bindings.push(key.to_string());
        }
        if let Ok(customer_key) = doc.get_str("customer_key") {
            conditions.push("customer_key = ?".to_string());
            bindings.push(customer_key.to_string());
        }
        if let Ok(status) = doc.get_str("status") {
            conditions.push("status = ?".to_string());
            bindings.push(status.to_string());
        }
        if let Ok(payment_method) = doc.get_str("payment_method") {
            conditions.push("payment_method = ?".to_string());
            bindings.push(payment_method.to_string());
        }
        if let Ok(is_credit) = doc.get_bool("is_credit") {
            conditions.push("is_credit = ?".to_string());
            bindings.push(if is_credit {
                "1".to_string()
            } else {
                "0".to_string()
            });
        }
        if let Ok(created_at_doc) = doc.get_document("created_at") {
            if let Ok(gte) = created_at_doc.get_datetime("$gte") {
                conditions.push("created_at >= ?".to_string());
                bindings.push(bson_to_iso(gte));
            }
            if let Ok(lt) = created_at_doc.get_datetime("$lt") {
                conditions.push("created_at < ?".to_string());
                bindings.push(bson_to_iso(lt));
            }
        }
        if let Ok(or_arr) = doc.get_array("$or") {
            let mut or_parts = Vec::new();
            for or_item in or_arr {
                if let Some(or_doc) = or_item.as_document() {
                    if let Ok(inv_num) = or_doc.get_document("invoice_number") {
                        if let Ok(regex) = inv_num.get_str("$regex") {
                            let clean = regex.trim_start_matches('^').trim_end_matches('$');
                            or_parts.push("invoice_number LIKE ?".to_string());
                            bindings.push(format!("%{clean}%"));
                        }
                    } else if let Ok(inv_exact) = or_doc.get_str("invoice_number") {
                        or_parts.push("invoice_number = ?".to_string());
                        bindings.push(inv_exact.to_string());
                    }
                    if let Ok(cname) = or_doc.get_document("customer_name_snapshot")
                        && let Ok(regex) = cname.get_str("$regex")
                    {
                        let clean = regex.trim_start_matches('^').trim_end_matches('$');
                        or_parts.push("customer_name_snapshot LIKE ?".to_string());
                        bindings.push(format!("%{clean}%"));
                    }
                    if let Ok(cphone) = or_doc.get_document("customer_phone_snapshot")
                        && let Ok(regex) = cphone.get_str("$regex")
                    {
                        let clean = regex.trim_start_matches('^').trim_end_matches('$');
                        or_parts.push("customer_phone_snapshot LIKE ?".to_string());
                        bindings.push(format!("%{clean}%"));
                    }
                    if let Ok(is_cred) = or_doc.get_bool("is_credit") {
                        or_parts.push("is_credit = ?".to_string());
                        bindings.push(if is_cred {
                            "1".to_string()
                        } else {
                            "0".to_string()
                        });
                    }
                    if let Ok(st) = or_doc.get_str("status") {
                        or_parts.push("status = ?".to_string());
                        bindings.push(st.to_string());
                    }
                }
            }
            if !or_parts.is_empty() {
                conditions.push(format!("({})", or_parts.join(" OR ")));
            }
        }
    }

    if let Ok(and_arr) = filter.get_array("$and") {
        for item in and_arr {
            if let Some(doc) = item.as_document() {
                process_clause(doc, &mut conditions, &mut bindings);
            }
        }
    } else {
        process_clause(filter, &mut conditions, &mut bindings);
    }

    (conditions.join(" AND "), bindings)
}

pub(crate) async fn insert_invoice(
    db: &Db,
    mut document: InvoiceDocument,
) -> AppResult<InvoiceDocument> {
    match db {
        Db::Mongo(db) => {
            let result = invoices(db).insert_one(&document).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let items_json =
                serde_json::to_string(&document.items).unwrap_or_else(|_| "[]".to_string());
            let split_payments_json = document
                .split_payments
                .as_ref()
                .map(|sp| serde_json::to_string(sp).unwrap_or_else(|_| "[]".to_string()));
            let shop_profile_json = serde_json::to_string(&document.shop_profile_snapshot)
                .unwrap_or_else(|_| "{}".to_string());
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);
            let voided_at_iso = document.voided_at.as_ref().map(bson_to_iso);
            let closed_at_iso = document.closed_at.as_ref().map(bson_to_iso);
            let status_str = serde_json::to_value(document.status)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "paid".to_string());

            sqlx::query(
                r#"
                INSERT INTO invoices (
                    key, id, invoice_number, customer_key, customer_name_snapshot,
                    customer_phone_snapshot, customer_address_snapshot, cashier_id,
                    cashier_name_snapshot, items, subtotal_cents, discount_type,
                    discount_value, discount_cents, total_cents, payment_method,
                    split_payments, is_credit, amount_received_cents, change_due_cents,
                    due_date, card_last4, card_ref, online_ref, online_note, status,
                    notes, shop_profile_snapshot, warranty_terms_snapshot, document_selection,
                    voided_at, voided_by, voided_reason, closed_at, closed_by,
                    refunded_cents, credit_note_count, version, created_at, updated_at
                ) VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                )
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.invoice_number)
            .bind(&document.customer_key)
            .bind(&document.customer_name_snapshot)
            .bind(&document.customer_phone_snapshot)
            .bind(&document.customer_address_snapshot)
            .bind(&document.cashier_id)
            .bind(&document.cashier_name_snapshot)
            .bind(&items_json)
            .bind(document.subtotal_cents)
            .bind(&document.discount_type)
            .bind(document.discount_value)
            .bind(document.discount_cents)
            .bind(document.total_cents)
            .bind(&document.payment_method)
            .bind(&split_payments_json)
            .bind(if document.is_credit { 1 } else { 0 })
            .bind(document.amount_received_cents)
            .bind(document.change_due_cents)
            .bind(&document.due_date)
            .bind(&document.card_last4)
            .bind(&document.card_ref)
            .bind(&document.online_ref)
            .bind(&document.online_note)
            .bind(&status_str)
            .bind(&document.notes)
            .bind(&shop_profile_json)
            .bind(&document.warranty_terms_snapshot)
            .bind(&document.document_selection)
            .bind(&voided_at_iso)
            .bind(&document.voided_by)
            .bind(&document.voided_reason)
            .bind(&closed_at_iso)
            .bind(&document.closed_by)
            .bind(document.refunded_cents)
            .bind(document.credit_note_count)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

pub(crate) async fn find_invoice_by_id(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<InvoiceDocument>> {
    match db {
        Db::Mongo(db) => Ok(invoices(db).find_one(doc! { "_id": id }).await?),
        Db::Sqlite(pool) => {
            let row_opt = sqlx::query("SELECT * FROM invoices WHERE id = ? AND deleted_at IS NULL")
                .bind(id.to_hex())
                .fetch_optional(pool)
                .await?;

            row_opt.map(|row| invoice_from_sqlite_row(&row)).transpose()
        }
    }
}

pub(crate) async fn find_invoice_by_key(db: &Db, key: &str) -> AppResult<Option<InvoiceDocument>> {
    match db {
        Db::Mongo(db) => Ok(invoices(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM invoices WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;

            row_opt.map(|row| invoice_from_sqlite_row(&row)).transpose()
        }
    }
}

pub(crate) async fn find_invoice_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<InvoiceDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_invoice_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    find_invoice_by_key(db, id_or_key).await
}

pub(crate) async fn list_invoices(
    db: &Db,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<InvoiceDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let total = invoices(db).count_documents(filter.clone()).await?;

            let mut cursor = invoices(db)
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .skip(skip)
                .limit(limit as i64)
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok((items, total))
        }
        Db::Sqlite(pool) => {
            let (where_clause, bindings) = build_invoice_where_clause(&filter);

            let count_sql = format!("SELECT COUNT(*) as total FROM invoices WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let count_row = count_query.fetch_one(pool).await?;
            let total: i64 = count_row.get("total");

            let select_sql = format!(
                "SELECT * FROM invoices WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            select_query = select_query.bind(limit as i64).bind(skip as i64);

            let rows = select_query.fetch_all(pool).await?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(invoice_from_sqlite_row(&row)?);
            }
            Ok((items, total as u64))
        }
    }
}

/// The one narrow mutator invoices ever get after creation — flips
/// `status` (and, for a void/close, the `voided_*`/`closed_*` fields) via
/// `$set`. No general "update an invoice" repository function exists (D3).
pub(crate) async fn update_invoice_status(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<InvoiceDocument>> {
    match db {
        Db::Mongo(db) => Ok(invoices(db)
            .find_one_and_update(
                doc! { "_id": id },
                doc! { "$set": set_doc, "$inc": { "version": 1 } },
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_hex = id.to_hex();
            let mut assignments = vec![
                "version = version + 1".to_string(),
                "updated_at = ?".to_string(),
            ];
            let now_iso = now_utc_iso();
            let mut bindings: Vec<String> = vec![now_iso];

            if let Ok(status) = set_doc.get_str("status") {
                assignments.push("status = ?".to_string());
                bindings.push(status.to_string());
            }
            if let Ok(voided_at) = set_doc.get_datetime("voided_at") {
                assignments.push("voided_at = ?".to_string());
                bindings.push(bson_to_iso(voided_at));
            }
            if let Ok(voided_by) = set_doc.get_str("voided_by") {
                assignments.push("voided_by = ?".to_string());
                bindings.push(voided_by.to_string());
            }
            if let Ok(voided_reason) = set_doc.get_str("voided_reason") {
                assignments.push("voided_reason = ?".to_string());
                bindings.push(voided_reason.to_string());
            }
            if let Ok(closed_at) = set_doc.get_datetime("closed_at") {
                assignments.push("closed_at = ?".to_string());
                bindings.push(bson_to_iso(closed_at));
            }
            if let Ok(closed_by) = set_doc.get_str("closed_by") {
                assignments.push("closed_by = ?".to_string());
                bindings.push(closed_by.to_string());
            }

            let sql = format!(
                "UPDATE invoices SET {} WHERE id = ?",
                assignments.join(", ")
            );
            let mut query = sqlx::query(&sql);
            for b in bindings {
                query = query.bind(b);
            }
            let res = query.bind(&id_hex).execute(pool).await?;

            if res.rows_affected() == 0 {
                return Ok(None);
            }

            find_invoice_by_id(db, id).await
        }
    }
}

pub(crate) async fn insert_payment(
    db: &Db,
    mut document: PaymentDocument,
) -> AppResult<PaymentDocument> {
    match db {
        Db::Mongo(db) => {
            let result = payments(db).insert_one(&document).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let recorded_at_iso = bson_to_iso(&document.recorded_at);
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);

            sqlx::query(
                r#"
                INSERT INTO payments (
                    key, id, invoice_key, amount_cents, payment_method, notes,
                    recorded_by_user_id, recorded_by_name_snapshot, recorded_at,
                    version, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.invoice_key)
            .bind(document.amount_cents)
            .bind(&document.payment_method)
            .bind(&document.notes)
            .bind(&document.recorded_by_user_id)
            .bind(&document.recorded_by_name_snapshot)
            .bind(&recorded_at_iso)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

pub(crate) async fn list_payments_for_invoice(
    db: &Db,
    invoice_key: &str,
) -> AppResult<Vec<PaymentDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = payments(db)
                .find(doc! { "invoice_key": invoice_key })
                .sort(doc! { "recorded_at": 1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM payments WHERE invoice_key = ? AND deleted_at IS NULL ORDER BY recorded_at ASC",
            )
            .bind(invoice_key)
            .fetch_all(pool)
            .await?;

            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(payment_from_sqlite_row(&row)?);
            }
            Ok(items)
        }
    }
}

/// Looks up a single payment by its unique model key (`pay_...`) — used by
/// `service::credit_notes::void_credit_note` to read back the original
/// amount/method of a refund leg it's about to reverse.
pub(crate) async fn find_payment_by_key(db: &Db, key: &str) -> AppResult<Option<PaymentDocument>> {
    match db {
        Db::Mongo(db) => Ok(payments(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM payments WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;

            row_opt.map(|row| payment_from_sqlite_row(&row)).transpose()
        }
    }
}

/// Used by `service::sale::cancel_invoice`'s guard (D8): cancellation is
/// blocked once more than the original sale-time payment(s) exist.
pub(crate) async fn count_payments_for_invoice(db: &Db, invoice_key: &str) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(payments(db)
            .count_documents(doc! { "invoice_key": invoice_key })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT COUNT(*) as cnt FROM payments WHERE invoice_key = ? AND deleted_at IS NULL",
            )
            .bind(invoice_key)
            .fetch_one(pool)
            .await?;
            let cnt: i64 = row.get("cnt");
            Ok(cnt as u64)
        }
    }
}

/// Everything `service::get_billing_stats` needs from `invoices`, computed in
/// one aggregation round-trip.
pub(crate) struct StatsAggregateResult {
    /// Sum of `total_cents` for invoices created in `[today_start, today_end)`.
    pub today_sales_cents: i64,
    /// Count of invoices created in that same window.
    pub today_invoice_count: u64,
    /// Sum of `total_cents` across ALL invoices, unscoped by date, where
    /// `is_credit == true` or `status` is `pending`/`partially_paid`.
    pub outstanding_credit_cents: i64,
}

/// Extracts a scalar `i64` field pushed by a `$group` facet branch, out of
/// that branch's array-of-one-document shape (`[{ "_id": null, field: N }]`,
/// empty array if the branch matched nothing).
fn i64_from_facet_branch(result: &Document, branch: &str, field: &str) -> i64 {
    result
        .get_array(branch)
        .ok()
        .and_then(|arr| arr.first())
        .and_then(|val| val.as_document())
        .and_then(|doc| {
            doc.get_i64(field)
                .ok()
                .or_else(|| doc.get_i32(field).ok().map(i64::from))
        })
        .unwrap_or(0)
}

/// Runs a single `$facet` aggregation over `invoices` to compute the Sales &
/// Invoices History screen's 4 KPI cards in one database round-trip:
/// today's sales sum + count, and the all-time outstanding-credit sum.
pub(crate) async fn aggregate_stats(
    db: &Db,
    today_start: DateTime<Utc>,
    today_end: DateTime<Utc>,
) -> AppResult<StatsAggregateResult> {
    match db {
        Db::Mongo(db) => {
            let pipeline = vec![doc! {
                "$facet": {
                    "today": [
                        {
                            "$match": {
                                "created_at": {
                                    "$gte": BsonDateTime::from_chrono(today_start),
                                    "$lt": BsonDateTime::from_chrono(today_end),
                                }
                            }
                        },
                        { "$group": { "_id": null, "sum": { "$sum": "$total_cents" }, "count": { "$sum": 1 } } }
                    ],
                    "outstanding": [
                        { "$match": { "status": { "$in": ["pending", "partially_paid"] } } },
                        { "$lookup": {
                            "from": "payments",
                            "localField": "key",
                            "foreignField": "invoice_key",
                            "as": "pmts",
                        } },
                        { "$project": {
                            "balance_due": {
                                "$max": [0, { "$subtract": ["$total_cents", { "$sum": "$pmts.amount_cents" }] }]
                            }
                        } },
                        { "$group": { "_id": null, "sum": { "$sum": "$balance_due" } } }
                    ],
                }
            }];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let Some(result) = cursor.try_next().await? else {
                return Ok(StatsAggregateResult {
                    today_sales_cents: 0,
                    today_invoice_count: 0,
                    outstanding_credit_cents: 0,
                });
            };

            let today_sales_cents = i64_from_facet_branch(&result, "today", "sum");
            let today_invoice_count = i64_from_facet_branch(&result, "today", "count") as u64;
            let outstanding_credit_cents = i64_from_facet_branch(&result, "outstanding", "sum");

            Ok(StatsAggregateResult {
                today_sales_cents,
                today_invoice_count,
                outstanding_credit_cents,
            })
        }
        Db::Sqlite(pool) => {
            let start_iso = today_start.to_rfc3339();
            let end_iso = today_end.to_rfc3339();

            let row = sqlx::query(
                r#"
                SELECT
                    COALESCE(SUM(CASE WHEN created_at >= ? AND created_at < ? THEN total_cents ELSE 0 END), 0) as today_sales_cents,
                    COUNT(CASE WHEN created_at >= ? AND created_at < ? THEN 1 ELSE NULL END) as today_invoice_count,
                    COALESCE(SUM(CASE WHEN status IN ('pending', 'partially_paid') THEN
                        MAX(0, total_cents - COALESCE((SELECT SUM(amount_cents) FROM payments WHERE payments.invoice_key = invoices.key AND payments.deleted_at IS NULL), 0))
                    ELSE 0 END), 0) as outstanding_credit_cents
                FROM invoices
                WHERE deleted_at IS NULL
                "#,
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_one(pool)
            .await?;

            let today_sales_cents: i64 = row.get("today_sales_cents");
            let today_invoice_count: i64 = row.get("today_invoice_count");
            let outstanding_credit_cents: i64 = row.get("outstanding_credit_cents");

            Ok(StatsAggregateResult {
                today_sales_cents,
                today_invoice_count: today_invoice_count as u64,
                outstanding_credit_cents,
            })
        }
    }
}
