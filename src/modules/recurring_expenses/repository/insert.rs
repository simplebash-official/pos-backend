use super::templates::templates;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::recurring_expenses::model::RecurringExpenseDocument;
use mongodb::bson::oid::ObjectId;

/// Inserts a new regular bill.
pub(crate) async fn insert(
    db: &Db,
    mut document: RecurringExpenseDocument,
) -> AppResult<RecurringExpenseDocument> {
    match db {
        Db::Mongo(db) => {
            let res = templates(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO recurring_expenses (
                    key, id, category_key, payee_key, amount_cents, due_day, remind_days_before,
                    paid_from, note, active, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.category_key)
            .bind(&document.payee_key)
            .bind(document.amount_cents)
            .bind(document.due_day)
            .bind(document.remind_days_before)
            .bind(document.paid_from.as_str())
            .bind(&document.note)
            .bind(i64::from(document.active))
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .execute(pool)
            .await?;
            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
