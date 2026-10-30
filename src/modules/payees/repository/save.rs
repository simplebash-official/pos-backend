use super::payees::payees;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;
use crate::modules::payees::model::PayeeDocument;
use mongodb::bson::doc;

/// Writes every editable field of an existing payee back (the service has already bumped `version`/`updated_at`).
pub(crate) async fn save(db: &Db, document: &PayeeDocument) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            payees(db)
                .replace_one(doc! { "key": &document.key }, document)
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            sqlx::query(
                "UPDATE payees SET name = ?, normalized_name = ?, phone = ?, active = ?, version = ?, updated_at = ?
                 WHERE key = ?",
            )
            .bind(&document.name)
            .bind(&document.normalized_name)
            .bind(&document.phone)
            .bind(i64::from(document.active))
            .bind(document.version)
            .bind(bson_to_iso(&document.updated_at))
            .bind(&document.key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}
