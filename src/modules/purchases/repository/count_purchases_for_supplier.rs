use super::purchases::purchases;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

/// Backs the `SUPPLIER_HAS_PURCHASES` delete guard in
/// `suppliers::service::delete_supplier`.
pub(crate) async fn count_purchases_for_supplier(db: &Db, supplier_key: &str) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(purchases(db)
            .count_documents(
                doc! { "supplier_key": supplier_key, "deleted_at": { "$exists": false } },
            )
            .await?),
        Db::Sqlite(pool) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM purchases WHERE supplier_key = ? AND deleted_at IS NULL",
            )
            .bind(supplier_key)
            .fetch_one(pool)
            .await?;

            Ok(count as u64)
        }
    }
}
