use super::invoice_from_sqlite_row::invoice_from_sqlite_row;
use super::invoices::invoices;
use crate::clients::db::Db;
use crate::clients::sqlite::SqlFilter;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;

use crate::modules::billing::model::InvoiceDocument;

use futures_util::TryStreamExt;

use mongodb::bson::{Document, doc};
use sqlx::Row;

fn build_invoice_where_clause(filter: &Document) -> SqlFilter {
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
        if let Ok(session_key) = doc.get_str("session_key") {
            conditions.push("session_key = ?".to_string());
            bindings.push(session_key.to_string());
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

    SqlFilter {
        clause: conditions.join(" AND "),
        bindings,
    }
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
            let SqlFilter {
                clause: where_clause,
                bindings,
            } = build_invoice_where_clause(&filter);

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
