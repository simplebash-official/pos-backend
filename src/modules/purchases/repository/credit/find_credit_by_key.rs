use super::credit_from_row::credit_from_row;
use super::credits::credits;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::purchases::model::SupplierCreditDocument;

use mongodb::bson::doc;

/// One credit (or voiding entry) by key.
pub(crate) async fn find_credit_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<SupplierCreditDocument>> {
    match db {
        Db::Mongo(db) => Ok(credits(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM supplier_credits WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(credit_from_row).transpose()
        }
    }
}
