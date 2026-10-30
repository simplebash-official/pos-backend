use super::credit_notes_collection::credit_notes_collection;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::billing::model::CreditNoteDocument;
use crate::modules::billing::repository::credit_note_from_sqlite_row;

use mongodb::bson::doc;

/// Look up a single credit note document by its unique model key (`cn_...`).
#[allow(dead_code)]
pub(crate) async fn find_credit_note_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<CreditNoteDocument>> {
    match db {
        Db::Mongo(db) => Ok(credit_notes_collection(db)
            .find_one(doc! { "key": key })
            .await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM credit_notes WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;

            row_opt
                .map(|row| credit_note_from_sqlite_row(&row))
                .transpose()
        }
    }
}
