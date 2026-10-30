use super::purchases::purchases;
use super::row_to_purchase_doc::row_to_purchase_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::purchases::model::PurchaseDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

pub(crate) async fn list_purchases_paginated(
    db: &Db,
    filter: mongodb::bson::Document,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<PurchaseDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let collection = purchases(db);
            let total = collection.count_documents(filter.clone()).await?;

            let mut cursor = collection
                .find(filter)
                .sort(doc! { "date": -1 })
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
            let supplier_key = filter.get_str("supplier_key").ok();
            let product_key = filter.get_str("product_key").ok();

            let mut count_query =
                String::from("SELECT COUNT(*) FROM purchases WHERE deleted_at IS NULL");
            let mut select_query = String::from("SELECT * FROM purchases WHERE deleted_at IS NULL");

            if supplier_key.is_some() {
                count_query.push_str(" AND supplier_key = ?");
                select_query.push_str(" AND supplier_key = ?");
            }
            if product_key.is_some() {
                count_query.push_str(" AND product_key = ?");
                select_query.push_str(" AND product_key = ?");
            }
            select_query.push_str(" ORDER BY date DESC LIMIT ? OFFSET ?");

            let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
            let mut select_q = sqlx::query(&select_query);

            if let Some(sk) = supplier_key {
                count_q = count_q.bind(sk);
                select_q = select_q.bind(sk);
            }
            if let Some(pk) = product_key {
                count_q = count_q.bind(pk);
                select_q = select_q.bind(pk);
            }

            select_q = select_q.bind(limit).bind(skip as i64);

            let total: i64 = count_q.fetch_one(pool).await?;
            let rows = select_q.fetch_all(pool).await?;
            let items = rows
                .iter()
                .map(row_to_purchase_doc)
                .collect::<AppResult<Vec<_>>>()?;

            Ok((items, total as u64))
        }
    }
}
