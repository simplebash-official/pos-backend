use super::expenses::expenses;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;
use crate::modules::expenses::model::ExpenseDocument;
use mongodb::bson::oid::ObjectId;

/// Inserts a new expense.
pub(crate) async fn insert_expense(
    unit: impl Into<Unit<'_>>,
    mut document: ExpenseDocument,
) -> AppResult<ExpenseDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let res = expenses(db).insert_one_in(&document, session).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO expenses (
                    key, id, expense_number, expense_date, category_key, payee_key, payee_name,
                    amount_cents, due_date, reference_no, note, recorded_by_user_id,
                    recorded_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.expense_number)
            .bind(&document.expense_date)
            .bind(&document.category_key)
            .bind(&document.payee_key)
            .bind(&document.payee_name)
            .bind(document.amount_cents)
            .bind(&document.due_date)
            .bind(&document.reference_no)
            .bind(&document.note)
            .bind(&document.recorded_by_user_id)
            .bind(&document.recorded_by_name_snapshot)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .execute(&mut *conn)
            .await?;
            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
