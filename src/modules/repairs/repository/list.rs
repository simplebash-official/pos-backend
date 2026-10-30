use super::repairs::repairs;
use super::row_to_repair_doc::row_to_repair_doc;
use crate::clients::db::Db;
use crate::clients::sqlite::SqlFilter;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;
use crate::modules::repairs::model::RepairDocument;

use futures_util::TryStreamExt;

use mongodb::bson::{Document, doc};

use sqlx::Row;

fn build_repair_where_clause(filter: &Document) -> SqlFilter {
    let mut conditions = vec!["deleted_at IS NULL".to_string()];
    let mut bindings = Vec::new();

    fn process_clause(doc: &Document, conditions: &mut Vec<String>, bindings: &mut Vec<String>) {
        if let Ok(key) = doc.get_str("key") {
            conditions.push("key = ?".to_string());
            bindings.push(key.to_string());
        }
        if let Ok(status) = doc.get_str("status") {
            conditions.push("status = ?".to_string());
            bindings.push(status.to_string());
        }
        if let Ok(customer_key) = doc.get_str("customer_key") {
            conditions.push("customer_key = ?".to_string());
            bindings.push(customer_key.to_string());
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
                    if let Ok(tick_doc) = or_doc.get_document("ticket_number") {
                        if let Ok(regex) = tick_doc.get_str("$regex") {
                            let clean = regex.trim_start_matches('^').trim_end_matches('$');
                            or_parts.push("ticket_number LIKE ?".to_string());
                            bindings.push(format!("%{clean}%"));
                        }
                    } else if let Ok(tick_exact) = or_doc.get_str("ticket_number") {
                        or_parts.push("ticket_number = ?".to_string());
                        bindings.push(tick_exact.to_string());
                    }

                    if let Ok(cname) = or_doc.get_document("customer_name")
                        && let Ok(regex) = cname.get_str("$regex")
                    {
                        let clean = regex.trim_start_matches('^').trim_end_matches('$');
                        or_parts.push("customer_name LIKE ?".to_string());
                        bindings.push(format!("%{clean}%"));
                    }

                    if let Ok(cphone) = or_doc.get_document("customer_phone")
                        && let Ok(regex) = cphone.get_str("$regex")
                    {
                        let clean = regex.trim_start_matches('^').trim_end_matches('$');
                        or_parts.push("customer_phone LIKE ?".to_string());
                        bindings.push(format!("%{clean}%"));
                    }

                    if let Ok(dev) = or_doc.get_document("device_model")
                        && let Ok(regex) = dev.get_str("$regex")
                    {
                        let clean = regex.trim_start_matches('^').trim_end_matches('$');
                        or_parts.push("device_model LIKE ?".to_string());
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

    SqlFilter {
        clause: conditions.join(" AND "),
        bindings,
    }
}

pub(crate) async fn list(
    db: &Db,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<RepairDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let mut effective_filter = filter;
            if !effective_filter.contains_key("deleted_at") {
                effective_filter.insert("deleted_at", doc! { "$exists": false });
            }

            let total = repairs(db)
                .count_documents(effective_filter.clone())
                .await?;

            let mut cursor = repairs(db)
                .find(effective_filter)
                .sort(doc! { "updated_at": -1 })
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
            } = build_repair_where_clause(&filter);

            let count_sql = format!("SELECT COUNT(*) as total FROM repairs WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let count_row = count_query.fetch_one(pool).await?;
            let total: i64 = count_row.get("total");

            let select_sql = format!(
                "SELECT * FROM repairs WHERE {where_clause} ORDER BY updated_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            select_query = select_query.bind(limit as i64).bind(skip as i64);

            let rows = select_query.fetch_all(pool).await?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(row_to_repair_doc(&row)?);
            }
            Ok((items, total as u64))
        }
    }
}
