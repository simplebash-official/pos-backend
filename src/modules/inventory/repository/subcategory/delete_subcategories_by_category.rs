use super::subcategories::subcategories;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

pub(crate) async fn delete_subcategories_by_category(
    db: &Db,
    category_key: &str,
) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(subcategories(db)
            .delete_many(doc! { "category_key": category_key })
            .await?
            .deleted_count),
        Db::Sqlite(pool) => {
            let res = sqlx::query("DELETE FROM subcategories WHERE category_key = ?")
                .bind(category_key)
                .execute(pool)
                .await?;
            Ok(res.rows_affected())
        }
    }
}
