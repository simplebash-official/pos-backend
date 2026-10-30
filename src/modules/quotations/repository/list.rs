use super::list_filter::ListFilter;
use super::quotations::quotations;
use super::row_to_doc::row_to_doc;
use crate::clients::db::Db;
use crate::clients::sqlite::SqlFilter;

use crate::core::error::AppResult;
use crate::core::utils::build_bson_regex;
use crate::domain::quotations::{STATUS_ACTIVE, STATUS_CANCELLED, STATUS_FULFILLED};
use crate::modules::quotations::model::QuotationDocument;
use futures_util::TryStreamExt;

use mongodb::bson::{Document, doc};
use sqlx::Row;

fn mongo_filter(filter: &ListFilter) -> Document {
    let mut clauses: Vec<Document> = Vec::new();
    if let Some(search) = &filter.search {
        let pattern = build_bson_regex(search);
        clauses.push(doc! { "$or": [
            { "quotation_number": { "$regex": pattern.clone() } },
            { "customer_name": { "$regex": pattern.clone() } },
            { "customer_phone": { "$regex": pattern } },
        ] });
    }
    match filter.status.as_deref() {
        Some("active") => clauses.push(
            doc! { "status": STATUS_ACTIVE, "valid_until": { "$gte": filter.today.as_str() } },
        ),
        Some("expired") => clauses.push(
            doc! { "status": STATUS_ACTIVE, "valid_until": { "$lt": filter.today.as_str() } },
        ),
        Some("fulfilled") => clauses.push(doc! { "status": STATUS_FULFILLED }),
        Some("cancelled") => clauses.push(doc! { "status": STATUS_CANCELLED }),
        _ => {}
    }
    if clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": clauses }
    }
}

fn sqlite_where(filter: &ListFilter) -> SqlFilter {
    let mut conditions: Vec<String> = vec!["1 = 1".to_string()];
    let mut bindings: Vec<String> = Vec::new();
    if let Some(search) = &filter.search {
        conditions.push(
            "(quotation_number LIKE ? OR customer_name LIKE ? OR customer_phone LIKE ?)"
                .to_string(),
        );
        let like = format!("%{search}%");
        bindings.extend([like.clone(), like.clone(), like]);
    }
    match filter.status.as_deref() {
        Some("active") => {
            conditions.push("status = ? AND valid_until >= ?".to_string());
            bindings.extend([STATUS_ACTIVE.to_string(), filter.today.clone()]);
        }
        Some("expired") => {
            conditions.push("status = ? AND valid_until < ?".to_string());
            bindings.extend([STATUS_ACTIVE.to_string(), filter.today.clone()]);
        }
        Some("fulfilled") => {
            conditions.push("status = ?".to_string());
            bindings.push(STATUS_FULFILLED.to_string());
        }
        Some("cancelled") => {
            conditions.push("status = ?".to_string());
            bindings.push(STATUS_CANCELLED.to_string());
        }
        _ => {}
    }
    SqlFilter {
        clause: conditions.join(" AND "),
        bindings,
    }
}

/// One page of quotations, newest first, plus the total matching count.
pub(crate) async fn list(
    db: &Db,
    filter: &ListFilter,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<QuotationDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let query = mongo_filter(filter);
            let total = quotations(db).count_documents(query.clone()).await?;
            let mut cursor = quotations(db)
                .find(query)
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
            } = sqlite_where(filter);

            let count_sql =
                format!("SELECT COUNT(*) AS total FROM quotations WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let total: i64 = count_query.fetch_one(pool).await?.get("total");

            let select_sql = format!(
                "SELECT * FROM quotations WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            let rows = select_query
                .bind(limit as i64)
                .bind(skip as i64)
                .fetch_all(pool)
                .await?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(row_to_doc(&row)?);
            }
            Ok((items, total as u64))
        }
    }
}
