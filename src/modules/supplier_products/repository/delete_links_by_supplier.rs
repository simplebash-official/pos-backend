use super::delete_links_where::delete_links_where;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

pub(crate) async fn delete_links_by_supplier(db: &Db, supplier_key: &str) -> AppResult<u64> {
    match db {
        Db::Mongo(mongo) => {
            delete_links_where(db, mongo, doc! { "supplier_key": supplier_key }).await
        }
        Db::Sqlite(pool) => {
            let res = sqlx::query("DELETE FROM supplier_products WHERE supplier_key = ?")
                .bind(supplier_key)
                .execute(pool)
                .await?;
            Ok(res.rows_affected())
        }
    }
}
