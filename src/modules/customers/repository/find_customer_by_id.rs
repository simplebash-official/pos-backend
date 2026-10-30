use super::customer_sqlite_row::CustomerSqliteRow;
use super::customers::customers;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::customers::model::CustomerDocument;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn find_customer_by_id(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<CustomerDocument>> {
    match db {
        Db::Mongo(db) => Ok(customers(db)
            .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let row = sqlx::query_as::<_, CustomerSqliteRow>(
                "SELECT * FROM customers WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&id_str)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(CustomerSqliteRow::into_document))
        }
    }
}
