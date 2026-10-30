use super::payees::payees;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::payees::model::PayeeDocument;
use mongodb::bson::oid::ObjectId;

/// Inserts a new payee.
pub(crate) async fn insert(db: &Db, mut document: PayeeDocument) -> AppResult<PayeeDocument> {
    match db {
        Db::Mongo(db) => {
            let res = payees(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO payees (key, id, payee_type, name, normalized_name, phone, active, version, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(document.payee_type.as_str())
            .bind(&document.name)
            .bind(&document.normalized_name)
            .bind(&document.phone)
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
