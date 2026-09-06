// Mongo and SQLite access for the `credit_notes` collection. Functions here never
// interpret a missing document as an error — they return `Option`/`Vec`/
// counts straight from the driver and leave the not-found -> AppError
// translation to `service`.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc, oid::ObjectId},
};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, now_utc_iso},
    },
    core::error::AppResult,
    modules::billing::{model::CreditNoteDocument, repository::credit_note_from_sqlite_row},
};

fn credit_notes(db: &Database) -> Collection<CreditNoteDocument> {
    db.collection("credit_notes")
}

fn build_credit_note_where_clause(filter: &Document) -> (String, Vec<String>) {
    let mut conditions = vec!["deleted_at IS NULL".to_string()];
    let mut bindings = Vec::new();

    fn process_clause(doc: &Document, conditions: &mut Vec<String>, bindings: &mut Vec<String>) {
        if let Ok(key) = doc.get_str("key") {
            conditions.push("key = ?".to_string());
            bindings.push(key.to_string());
        }
        if let Ok(inv_key) = doc.get_str("invoice_key") {
            conditions.push("invoice_key = ?".to_string());
            bindings.push(inv_key.to_string());
        }
        if let Ok(cust_key) = doc.get_str("customer_key") {
            conditions.push("customer_key = ?".to_string());
            bindings.push(cust_key.to_string());
        }
        if let Ok(status) = doc.get_str("status") {
            conditions.push("status = ?".to_string());
            bindings.push(status.to_string());
        } else if let Ok(status_doc) = doc.get_document("status")
            && let Ok(ne) = status_doc.get_str("$ne")
        {
            conditions.push("status != ?".to_string());
            bindings.push(ne.to_string());
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
                    if let Ok(cn_doc) = or_doc.get_document("credit_note_number") {
                        if let Ok(regex) = cn_doc.get_str("$regex") {
                            let clean = regex.trim_start_matches('^').trim_end_matches('$');
                            or_parts.push("credit_note_number LIKE ?".to_string());
                            bindings.push(format!("%{clean}%"));
                        }
                    } else if let Ok(cn_exact) = or_doc.get_str("credit_note_number") {
                        or_parts.push("credit_note_number = ?".to_string());
                        bindings.push(cn_exact.to_string());
                    }

                    if let Ok(inv_doc) = or_doc.get_document("invoice_number") {
                        if let Ok(regex) = inv_doc.get_str("$regex") {
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

/// Inserts a new credit note document and populates its auto-generated `_id`.
pub(crate) async fn insert_credit_note(
    db: &Db,
    mut document: CreditNoteDocument,
) -> AppResult<CreditNoteDocument> {
    match db {
        Db::Mongo(db) => {
            let result = credit_notes(db).insert_one(&document).await?;
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
            let returned_items_json = serde_json::to_string(&document.returned_items)
                .unwrap_or_else(|_| "[]".to_string());
            let exchange_items_json = serde_json::to_string(&document.exchange_items)
                .unwrap_or_else(|_| "[]".to_string());
            let refund_breakdown_json = serde_json::to_string(&document.refund_breakdown)
                .unwrap_or_else(|_| "[]".to_string());
            let refund_payment_keys_json = serde_json::to_string(&document.refund_payment_keys)
                .unwrap_or_else(|_| "[]".to_string());
            let status_str = serde_json::to_value(document.status)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "approved".to_string());

            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);
            let voided_at_iso = document.voided_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO credit_notes (
                    key, id, credit_note_number, invoice_key, invoice_number, no_receipt,
                    customer_key, customer_name_snapshot, cashier_id, cashier_name_snapshot,
                    returned_items, exchange_items, exchange_reference, return_subtotal_cents,
                    exchange_subtotal_cents, net_refund_cents, refund_cash_cents,
                    balance_reduction_cents, refund_breakdown, refund_payment_keys, status,
                    is_manager_override, override_approved_by, override_reason, notes,
                    voided_at, voided_by, voided_reason, version, created_at, updated_at
                ) VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                )
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.credit_note_number)
            .bind(&document.invoice_key)
            .bind(&document.invoice_number)
            .bind(if document.no_receipt { 1 } else { 0 })
            .bind(&document.customer_key)
            .bind(&document.customer_name_snapshot)
            .bind(&document.cashier_id)
            .bind(&document.cashier_name_snapshot)
            .bind(&returned_items_json)
            .bind(&exchange_items_json)
            .bind(&document.exchange_reference)
            .bind(document.return_subtotal_cents)
            .bind(document.exchange_subtotal_cents)
            .bind(document.net_refund_cents)
            .bind(document.refund_cash_cents)
            .bind(document.balance_reduction_cents)
            .bind(&refund_breakdown_json)
            .bind(&refund_payment_keys_json)
            .bind(&status_str)
            .bind(if document.is_manager_override { 1 } else { 0 })
            .bind(&document.override_approved_by)
            .bind(&document.override_reason)
            .bind(&document.notes)
            .bind(&voided_at_iso)
            .bind(&document.voided_by)
            .bind(&document.voided_reason)
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

/// Look up a single credit note document by its Mongo ObjectId.
pub(crate) async fn find_credit_note_by_id(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<CreditNoteDocument>> {
    match db {
        Db::Mongo(db) => Ok(credit_notes(db).find_one(doc! { "_id": id }).await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM credit_notes WHERE id = ? AND deleted_at IS NULL")
                    .bind(id.to_hex())
                    .fetch_optional(pool)
                    .await?;

            row_opt
                .map(|row| credit_note_from_sqlite_row(&row))
                .transpose()
        }
    }
}

/// Look up a single credit note document by its unique model key (`cn_...`).
#[allow(dead_code)]
pub(crate) async fn find_credit_note_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<CreditNoteDocument>> {
    match db {
        Db::Mongo(db) => Ok(credit_notes(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM credit_notes WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;

            row_opt
                .map(|row| credit_note_from_sqlite_row(&row))
                .transpose()
        }
    }
}

/// Look up a single credit note document by either its hex ObjectId or unique model key.
pub(crate) async fn find_credit_note_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<CreditNoteDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_credit_note_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    match db {
        Db::Mongo(db) => Ok(credit_notes(db)
            .find_one(doc! {
                "$or": [
                    { "key": id_or_key },
                    { "credit_note_number": id_or_key },
                ]
            })
            .await?),
        Db::Sqlite(pool) => {
            let row_opt = sqlx::query(
                "SELECT * FROM credit_notes WHERE (key = ? OR credit_note_number = ?) AND deleted_at IS NULL",
            )
            .bind(id_or_key)
            .bind(id_or_key)
            .fetch_optional(pool)
            .await?;

            row_opt
                .map(|row| credit_note_from_sqlite_row(&row))
                .transpose()
        }
    }
}

/// Lists credit note documents matching a filter, sorted newest first with pagination.
pub(crate) async fn list_credit_notes(
    db: &Db,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<CreditNoteDocument>, u64)> {
    match db {
        Db::Mongo(mongo_db) => {
            let total = credit_notes(mongo_db)
                .count_documents(filter.clone())
                .await?;

            let mut cursor = credit_notes(mongo_db)
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
            let (where_clause, bindings) = build_credit_note_where_clause(&filter);

            let count_sql =
                format!("SELECT COUNT(*) as total FROM credit_notes WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let count_row = count_query.fetch_one(pool).await?;
            let total: i64 = count_row.get("total");

            let select_sql = format!(
                "SELECT * FROM credit_notes WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            select_query = select_query.bind(limit as i64).bind(skip as i64);

            let rows = select_query.fetch_all(pool).await?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(credit_note_from_sqlite_row(&row)?);
            }
            Ok((items, total as u64))
        }
    }
}

/// Returns the count of credit note documents matching the specified filter.
pub(crate) async fn count_credit_notes(db: &Db, filter: Document) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(credit_notes(db).count_documents(filter).await?),
        Db::Sqlite(pool) => {
            let (where_clause, bindings) = build_credit_note_where_clause(&filter);
            let count_sql =
                format!("SELECT COUNT(*) as total FROM credit_notes WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let count_row = count_query.fetch_one(pool).await?;
            let total: i64 = count_row.get("total");
            Ok(total as u64)
        }
    }
}

/// Count of non-voided credit notes against an invoice — the guard behind
/// `service::sale::close_invoice`'s "no open credit note" rule.
pub(crate) async fn count_open_credit_notes_for_invoice(
    db: &Db,
    invoice_key: &str,
) -> AppResult<u64> {
    count_credit_notes(
        db,
        doc! { "invoice_key": invoice_key, "status": { "$ne": "voided" } },
    )
    .await
}

/// The one narrow mutator credit notes ever get after creation — flips
/// `status` (and, for a void, the `voided_*` fields) via `$set`. No general
/// "update a credit note" repository function exists (append-only, same
/// posture as invoices).
pub(crate) async fn update_credit_note_status(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<CreditNoteDocument>> {
    match db {
        Db::Mongo(db) => Ok(credit_notes(db)
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

            let sql = format!(
                "UPDATE credit_notes SET {} WHERE id = ?",
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

            find_credit_note_by_id(db, id).await
        }
    }
}
