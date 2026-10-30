use super::credit_from_row::credit_from_row;
use super::credits::credits;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::purchases::model::SupplierCreditDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Newest first, optionally one supplier's and/or one receipt's, with the total for paging.
pub(crate) async fn list_credits_paginated(
    db: &Db,
    supplier_key: Option<&str>,
    receipt_key: Option<&str>,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<SupplierCreditDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let mut filter = doc! {};
            if let Some(key) = supplier_key {
                filter.insert("supplier_key", key);
            }
            if let Some(key) = receipt_key {
                filter.insert("receipt_key", key);
            }
            let collection = credits(db);
            let total = collection.count_documents(filter.clone()).await?;
            let mut cursor = collection
                .find(filter)
                .sort(doc! { "created_at": -1, "key": -1 })
                .skip(skip)
                .limit(limit)
                .await?;
            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok((items, total))
        }
        Db::Sqlite(pool) => {
            let mut conditions = Vec::new();
            if supplier_key.is_some() {
                conditions.push("supplier_key = ?");
            }
            if receipt_key.is_some() {
                conditions.push("receipt_key = ?");
            }
            let filter = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };
            let count_sql = format!("SELECT COUNT(*) FROM supplier_credits{filter}");
            let select_sql = format!(
                "SELECT * FROM supplier_credits{filter} ORDER BY created_at DESC, key DESC LIMIT ? OFFSET ?"
            );
            let mut count = sqlx::query_scalar::<_, i64>(&count_sql);
            let mut select = sqlx::query(&select_sql);
            for value in [supplier_key, receipt_key].into_iter().flatten() {
                count = count.bind(value);
                select = select.bind(value);
            }
            let total = count.fetch_one(pool).await?;
            let rows = select.bind(limit).bind(skip as i64).fetch_all(pool).await?;
            Ok((
                rows.iter().map(credit_from_row).collect::<AppResult<_>>()?,
                total as u64,
            ))
        }
    }
}
