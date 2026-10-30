use super::openings::openings;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;
use crate::modules::accounts::model::AccountOpeningDocument;
use mongodb::bson::doc;

/// Writes a changed opening balance back (the service bumped `version`/`updated_at`).
pub(crate) async fn save(db: &Db, document: &AccountOpeningDocument) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            openings(db)
                .replace_one(doc! { "key": &document.key }, document)
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            sqlx::query(
                "UPDATE account_openings SET opening_balance_cents = ?, as_of_date = ?,
                    set_by_name_snapshot = ?, version = ?, updated_at = ? WHERE key = ?",
            )
            .bind(document.opening_balance_cents)
            .bind(&document.as_of_date)
            .bind(&document.set_by_name_snapshot)
            .bind(document.version)
            .bind(bson_to_iso(&document.updated_at))
            .bind(&document.key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}
