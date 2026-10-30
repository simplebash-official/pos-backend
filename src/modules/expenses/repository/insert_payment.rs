use super::payments::payments;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;
use crate::modules::expenses::model::ExpensePaymentDocument;
use mongodb::bson::oid::ObjectId;

/// Inserts a payment against an expense.
pub(crate) async fn insert_payment(
    unit: impl Into<Unit<'_>>,
    mut document: ExpensePaymentDocument,
) -> AppResult<ExpensePaymentDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let res = payments(db).insert_one_in(&document, session).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO expense_payments (
                    key, id, expense_key, amount_cents, paid_from, paid_on, reference_no, note,
                    recorded_by_user_id, recorded_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.expense_key)
            .bind(document.amount_cents)
            .bind(document.paid_from.as_str())
            .bind(&document.paid_on)
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
