use super::recoveries::recoveries;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::balances::model::AdvanceRecoveryDocument;
use mongodb::bson::oid::ObjectId;

/// Inserts a recovery.
pub(crate) async fn insert_recovery(
    db: &Db,
    mut document: AdvanceRecoveryDocument,
) -> AppResult<AdvanceRecoveryDocument> {
    match db {
        Db::Mongo(db) => {
            let res = recoveries(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO advance_recoveries (
                    key, id, employee_key, amount_cents, recovered_on, note, recorded_by_user_id,
                    recorded_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.employee_key)
            .bind(document.amount_cents)
            .bind(&document.recovered_on)
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
