use super::incomes::incomes;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::incomes::model::IncomeDocument;
use mongodb::bson::oid::ObjectId;

/// Inserts a new income.
pub(crate) async fn insert_income(
    db: &Db,
    mut document: IncomeDocument,
) -> AppResult<IncomeDocument> {
    match db {
        Db::Mongo(db) => {
            let res = incomes(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO incomes (
                    key, id, income_number, income_date, category_key, payee_key, payee_name,
                    amount_cents, received_into, reference_no, note, recorded_by_user_id,
                    recorded_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.income_number)
            .bind(&document.income_date)
            .bind(&document.category_key)
            .bind(&document.payee_key)
            .bind(&document.payee_name)
            .bind(document.amount_cents)
            .bind(document.received_into.as_str())
            .bind(&document.reference_no)
            .bind(&document.note)
            .bind(&document.recorded_by_user_id)
            .bind(&document.recorded_by_name_snapshot)
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
