use super::templates::templates;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;
use crate::modules::recurring_expenses::model::RecurringExpenseDocument;
use mongodb::bson::doc;

/// Writes the editable fields of an existing regular bill back (the service bumped `version`/`updated_at`).
pub(crate) async fn save(db: &Db, document: &RecurringExpenseDocument) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            templates(db)
                .replace_one(doc! { "key": &document.key }, document)
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            sqlx::query(
                "UPDATE recurring_expenses SET amount_cents = ?, due_day = ?, remind_days_before = ?,
                    paid_from = ?, note = ?, active = ?, version = ?, updated_at = ? WHERE key = ?",
            )
            .bind(document.amount_cents)
            .bind(document.due_day)
            .bind(document.remind_days_before)
            .bind(document.paid_from.as_str())
            .bind(&document.note)
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
