use super::supplier_sqlite_row::SupplierSqliteRow;
use super::suppliers::suppliers;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::suppliers::model::SupplierDocument;

use mongodb::bson::doc;

pub(crate) async fn find_supplier_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<SupplierDocument>> {
    match db {
        Db::Mongo(db) => Ok(suppliers(db)
            .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query_as::<_, SupplierSqliteRow>(
                "SELECT * FROM suppliers WHERE key = ? AND deleted_at IS NULL",
            )
            .bind(key)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(SupplierSqliteRow::into_document))
        }
    }
}
