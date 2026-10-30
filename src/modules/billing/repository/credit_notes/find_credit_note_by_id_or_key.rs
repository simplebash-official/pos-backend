use super::credit_notes_collection::credit_notes_collection;
use super::find_credit_note_by_id::find_credit_note_by_id;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::billing::model::CreditNoteDocument;
use crate::modules::billing::repository::credit_note_from_sqlite_row;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

/// Look up a single credit note document by either its hex ObjectId or unique model key.
pub(crate) async fn find_credit_note_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<CreditNoteDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_credit_note_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    match db {
        Db::Mongo(db) => Ok(credit_notes_collection(db)
            .find_one(doc! {
                "$or": [
                    { "key": id_or_key },
                    { "credit_note_number": id_or_key },
                ]
            })
            .await?),
        Db::Sqlite(pool) => {
            let row_opt = sqlx::query(
                "SELECT * FROM credit_notes WHERE (key = ? OR credit_note_number = ?) AND deleted_at IS NULL",
            )
            .bind(id_or_key)
            .bind(id_or_key)
            .fetch_optional(pool)
            .await?;

            row_opt
                .map(|row| credit_note_from_sqlite_row(&row))
                .transpose()
        }
    }
}
