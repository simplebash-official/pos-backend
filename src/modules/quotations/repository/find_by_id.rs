use super::quotations::quotations;
use super::row_to_doc::row_to_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::quotations::model::QuotationDocument;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn find_by_id(db: &Db, id: ObjectId) -> AppResult<Option<QuotationDocument>> {
    match db {
        Db::Mongo(db) => Ok(quotations(db).find_one(doc! { "_id": id }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM quotations WHERE id = ?")
                .bind(id.to_hex())
                .fetch_optional(pool)
                .await?;
            row.map(|r| row_to_doc(&r)).transpose()
        }
    }
}
