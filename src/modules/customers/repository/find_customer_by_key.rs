use super::customer_sqlite_row::CustomerSqliteRow;
use super::customers::customers;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::customers::model::CustomerDocument;

use mongodb::bson::doc;

pub(crate) async fn find_customer_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<CustomerDocument>> {
    match db {
        Db::Mongo(db) => Ok(customers(db)
            .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query_as::<_, CustomerSqliteRow>(
                "SELECT * FROM customers WHERE key = ? AND deleted_at IS NULL",
            )
            .bind(key)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(CustomerSqliteRow::into_document))
        }
    }
}
