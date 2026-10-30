use super::receipt_from_row::receipt_from_row;
use super::receipts::receipts;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::purchases::model::PurchaseReceiptDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Newest first, optionally one supplier's, with the total for paging.
pub(crate) async fn list_receipts_paginated(
    db: &Db,
    supplier_key: Option<&str>,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<PurchaseReceiptDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let filter = match supplier_key {
                Some(key) => doc! { "supplier_key": key },
                None => doc! {},
            };
            let collection = receipts(db);
            let total = collection.count_documents(filter.clone()).await?;
            let mut cursor = collection
                .find(filter)
                .sort(doc! { "date": -1, "created_at": -1 })
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
            let filter = if supplier_key.is_some() {
                " WHERE supplier_key = ?"
            } else {
                ""
            };
            let count_sql = format!("SELECT COUNT(*) FROM purchase_receipts{filter}");
            let select_sql = format!(
                "SELECT * FROM purchase_receipts{filter} ORDER BY date DESC, created_at DESC LIMIT ? OFFSET ?"
            );
            let mut count = sqlx::query_scalar::<_, i64>(&count_sql);
            let mut select = sqlx::query(&select_sql);
            if let Some(key) = supplier_key {
                count = count.bind(key);
                select = select.bind(key);
            }
            let total = count.fetch_one(pool).await?;
            let rows = select.bind(limit).bind(skip as i64).fetch_all(pool).await?;
            Ok((rows.iter().map(receipt_from_row).collect(), total as u64))
        }
    }
}
