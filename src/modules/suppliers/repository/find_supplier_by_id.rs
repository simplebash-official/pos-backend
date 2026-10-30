use super::supplier_sqlite_row::SupplierSqliteRow;
use super::suppliers::suppliers;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::suppliers::model::SupplierDocument;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn find_supplier_by_id(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<SupplierDocument>> {
    match db {
        Db::Mongo(db) => Ok(suppliers(db)
            .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let row = sqlx::query_as::<_, SupplierSqliteRow>(
                "SELECT * FROM suppliers WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&id_str)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(SupplierSqliteRow::into_document))
        }
    }
}
