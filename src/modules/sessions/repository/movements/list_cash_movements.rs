use super::cash_movements_col::cash_movements_col;
use super::movement_from_sqlite_row::movement_from_sqlite_row;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::sessions::CashMovementListQuery;
use crate::modules::sessions::model::CashMovementDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

use sqlx::Row;

pub async fn list_cash_movements(
    db: &Db,
    query: &CashMovementListQuery,
) -> AppResult<(Vec<CashMovementDocument>, u64)> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(50).min(200);
    let skip = (page - 1) * limit;

    match db {
        Db::Mongo(mongo_db) => {
            let mut filter = doc! {};
            if let Some(session_key) = &query.session_key {
                filter.insert("session_key", session_key);
            }
            if let Some(terminal_id) = &query.terminal_id {
                filter.insert("terminal_id", terminal_id);
            }
            if let Some(movement_type) = &query.movement_type {
                filter.insert("movement_type", movement_type);
            }
            if let Some(category_keys) = &query.category_keys {
                filter.insert("category_key", doc! { "$in": category_keys });
            }
            if let Some(payee_key) = &query.payee_key {
                filter.insert("payee_key", payee_key);
            }
            if let Some(recorded_late) = query.recorded_late {
                filter.insert("recorded_late", recorded_late);
            }
            if let Some(from) = &query.from {
                filter.insert("created_at", doc! { "$gte": from });
            }
            if let Some(to) = &query.to {
                filter
                    .entry("created_at".to_string())
                    .or_insert_with(|| doc! {}.into());
                if let Ok(d) = filter.get_document_mut("created_at") {
                    d.insert("$lte", to);
                }
            }

            let total = cash_movements_col(mongo_db)
                .count_documents(filter.clone())
                .await?;
            let mut cursor = cash_movements_col(mongo_db)
                .find(filter)
                .sort(doc! { "created_at": -1 })
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

            if let Some(session_key) = &query.session_key {
                conditions.push("session_key = ?".to_string());
                bindings.push(session_key.clone());
            }
            if let Some(terminal_id) = &query.terminal_id {
                conditions.push("terminal_id = ?".to_string());
                bindings.push(terminal_id.clone());
            }
            if let Some(movement_type) = &query.movement_type {
                conditions.push("movement_type = ?".to_string());
                bindings.push(movement_type.clone());
            }
            if let Some(category_keys) = &query.category_keys {
                let placeholders = category_keys
                    .iter()
                    .map(|_| "?")
                    .collect::<Vec<_>>()
                    .join(",");
                conditions.push(format!("category_key IN ({placeholders})"));
                bindings.extend(category_keys.iter().cloned());
            }
            if let Some(payee_key) = &query.payee_key {
                conditions.push("payee_key = ?".to_string());
                bindings.push(payee_key.clone());
            }
            if let Some(recorded_late) = query.recorded_late {
                conditions.push("recorded_late = ?".to_string());
                bindings.push(if recorded_late {
                    "1".to_string()
                } else {
                    "0".to_string()
                });
            }
            if let Some(from) = &query.from {
                conditions.push("created_at >= ?".to_string());
                bindings.push(from.clone());
            }
            if let Some(to) = &query.to {
                conditions.push("created_at <= ?".to_string());
                bindings.push(to.clone());
            }

            let where_clause = conditions.join(" AND ");
            let count_sql =
                format!("SELECT COUNT(*) as total FROM cash_movements WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let count_row = count_query.fetch_one(pool).await?;
            let total: i64 = count_row.get("total");

            let select_sql = format!(
                "SELECT * FROM cash_movements WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            select_query = select_query.bind(limit as i64).bind(skip as i64);

            let rows = select_query.fetch_all(pool).await?;
            let mut results = Vec::with_capacity(rows.len());
            for r in rows {
                results.push(movement_from_sqlite_row(&r)?);
            }
            Ok((results, total as u64))
        }
    }
}
