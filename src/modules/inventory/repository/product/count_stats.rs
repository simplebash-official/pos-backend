use super::products::products;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

pub(crate) async fn count_stats(db: &Db) -> AppResult<(u64, u64)> {
    match db {
        Db::Mongo(db) => {
            let total_items = products(db)
                .count_documents(doc! { "deleted_at": { "$exists": false } })
                .await?;
            let low_stock_alerts = products(db)
                .count_documents(doc! {
                    "deleted_at": { "$exists": false },
                    "$expr": { "$lte": ["$stock_quantity", "$min_stock_threshold"] }
                })
                .await?;
            Ok((total_items, low_stock_alerts))
        }
        Db::Sqlite(pool) => {
            let total_items: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE deleted_at IS NULL")
                    .fetch_one(pool)
                    .await?;

            let low_stock_alerts: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM products WHERE deleted_at IS NULL AND stock_quantity <= min_stock_threshold",
            )
            .fetch_one(pool)
            .await?;

            Ok((total_items as u64, low_stock_alerts as u64))
        }
    }
}
