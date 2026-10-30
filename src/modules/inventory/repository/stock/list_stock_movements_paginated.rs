use super::row_to_stock_movement_doc::row_to_stock_movement_doc;
use super::stock_movements::stock_movements;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::model::StockMovementDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

pub(crate) async fn list_stock_movements_paginated(
    db: &Db,
    filter: mongodb::bson::Document,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<StockMovementDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let collection = stock_movements(db);
            let total = collection.count_documents(filter.clone()).await?;

            let mut cursor = collection
                .find(filter)
                .sort(doc! { "created_at": -1 })
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
            let product_id = filter.get_object_id("product_id").ok();
            let movement_type = filter.get_str("movement_type").ok();

            let mut count_query =
                String::from("SELECT COUNT(*) FROM stock_movements WHERE deleted_at IS NULL");
            let mut select_query =
                String::from("SELECT * FROM stock_movements WHERE deleted_at IS NULL");

            if product_id.is_some() {
                count_query.push_str(" AND product_id = ?");
                select_query.push_str(" AND product_id = ?");
            }
            if movement_type.is_some() {
                count_query.push_str(" AND movement_type = ?");
                select_query.push_str(" AND movement_type = ?");
            }

            select_query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

            let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
            let mut select_q = sqlx::query(&select_query);

            if let Some(pid) = product_id {
                let pid_hex = pid.to_hex();
                count_q = count_q.bind(pid_hex.clone());
                select_q = select_q.bind(pid_hex);
            }
            if let Some(mt) = movement_type {
                count_q = count_q.bind(mt);
                select_q = select_q.bind(mt);
            }

            select_q = select_q.bind(limit).bind(skip as i64);

            let total: i64 = count_q.fetch_one(pool).await?;
            let rows = select_q.fetch_all(pool).await?;
            let items = rows
                .iter()
                .map(row_to_stock_movement_doc)
                .collect::<AppResult<Vec<_>>>()?;

            Ok((items, total as u64))
        }
    }
}
