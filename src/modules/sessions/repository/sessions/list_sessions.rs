use super::session_from_sqlite_row::session_from_sqlite_row;
use super::sessions_col::sessions_col;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::sessions::SessionListQuery;
use crate::modules::sessions::model::RegisterSessionDocument;
use futures_util::TryStreamExt;

use mongodb::bson::doc;
use sqlx::Row;

pub async fn list_sessions(
    db: &Db,
    query: &SessionListQuery,
) -> AppResult<(Vec<RegisterSessionDocument>, u64)> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).min(100);
    let skip = (page - 1) * limit;

    match db {
        Db::Mongo(mongo_db) => {
            let mut filter = doc! {};
            if let Some(terminal_id) = &query.terminal_id {
                filter.insert("terminal_id", terminal_id);
            }
            if let Some(cashier_id) = &query.cashier_id {
                filter.insert("cashier_id", cashier_id);
            }
            if let Some(status) = &query.status {
                filter.insert("status", status);
            }
            if let Some(search) = &query.search {
                let s = search.trim();
                if !s.is_empty() {
                    let regex = crate::core::utils::build_bson_regex(s);
                    filter.insert(
                        "$or",
                        vec![
                            doc! { "session_number": &regex },
                            doc! { "cashier_name_snapshot": &regex },
                            doc! { "opening_note": &regex },
                            doc! { "closing_note": &regex },
                        ],
                    );
                }
            }
            if let Some(from) = &query.from {
                filter.insert("opened_at", doc! { "$gte": from });
            }
            if let Some(to) = &query.to {
                filter
                    .entry("opened_at".to_string())
                    .or_insert_with(|| doc! {}.into());
                if let Ok(d) = filter.get_document_mut("opened_at") {
                    d.insert("$lte", to);
                }
            }

            let total = sessions_col(mongo_db)
                .count_documents(filter.clone())
                .await?;
            let mut cursor = sessions_col(mongo_db)
                .find(filter)
                .sort(doc! { "opened_at": -1 })
                .skip(skip)
                .limit(limit as i64)
                .await?;

            let mut results = Vec::new();
            while let Some(item) = cursor.try_next().await? {
                results.push(item);
            }
            Ok((results, total))
        }
        Db::Sqlite(pool) => {
            let mut conditions = vec!["deleted_at IS NULL".to_string()];
            let mut bindings: Vec<String> = Vec::new();

            if let Some(terminal_id) = &query.terminal_id {
                conditions.push("terminal_id = ?".to_string());
                bindings.push(terminal_id.clone());
            }
            if let Some(cashier_id) = &query.cashier_id {
                conditions.push("cashier_id = ?".to_string());
                bindings.push(cashier_id.clone());
            }
            if let Some(status) = &query.status {
                conditions.push("status = ?".to_string());
                bindings.push(status.clone());
            }
            if let Some(search) = &query.search {
                let s = search.trim();
                if !s.is_empty() {
                    let pattern = format!("%{s}%");
                    conditions.push("(session_number LIKE ? OR cashier_name_snapshot LIKE ? OR opening_note LIKE ? OR closing_note LIKE ?)".to_string());
                    bindings.push(pattern.clone());
                    bindings.push(pattern.clone());
                    bindings.push(pattern.clone());
                    bindings.push(pattern);
                }
            }
            if let Some(from) = &query.from {
                conditions.push("opened_at >= ?".to_string());
                bindings.push(from.clone());
            }
            if let Some(to) = &query.to {
                conditions.push("opened_at <= ?".to_string());
                bindings.push(to.clone());
            }

            let where_clause = conditions.join(" AND ");
            let count_sql =
                format!("SELECT COUNT(*) as total FROM register_sessions WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let count_row = count_query.fetch_one(pool).await?;
            let total: i64 = count_row.get("total");

            let select_sql = format!(
                "SELECT * FROM register_sessions WHERE {where_clause} ORDER BY opened_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            select_query = select_query.bind(limit as i64).bind(skip as i64);

            let rows = select_query.fetch_all(pool).await?;
            let mut results = Vec::with_capacity(rows.len());
            for r in rows {
                results.push(session_from_sqlite_row(&r)?);
            }
            Ok((results, total as u64))
        }
    }
}
