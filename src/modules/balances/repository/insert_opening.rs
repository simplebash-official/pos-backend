use super::openings::openings;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::balances::model::BalanceOpeningDocument;
use mongodb::bson::oid::ObjectId;

/// Inserts the first opening of a supplier or staff member.
pub(crate) async fn insert_opening(
    db: &Db,
    mut document: BalanceOpeningDocument,
) -> AppResult<BalanceOpeningDocument> {
    match db {
        Db::Mongo(db) => {
            let res = openings(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO balance_openings (
                    key, id, subject_kind, subject_key, opening_balance_cents, as_of_date,
                    set_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(document.subject_kind.as_str())
            .bind(&document.subject_key)
            .bind(document.opening_balance_cents)
            .bind(&document.as_of_date)
            .bind(&document.set_by_name_snapshot)
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
