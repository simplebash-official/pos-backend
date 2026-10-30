use super::users::users;
use crate::clients::db::Db;
use crate::clients::sqlite::generate_id_hex;

use crate::core::error::AppResult;

use crate::modules::users::model::UserDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert_user(db: &Db, mut document: UserDocument) -> AppResult<UserDocument> {
    match db {
        Db::Mongo(db) => {
            let result = users(db).insert_one(&document).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id_str = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let created_str = document.created_at.to_chrono().to_rfc3339();
            let updated_str = document.updated_at.to_chrono().to_rfc3339();

            sqlx::query(
                r#"
                INSERT INTO users (key, id, name, username, password_hash, pin_hash, role, employee_key, is_active, version, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 1, $10, $11)
                "#,
            )
            .bind(&document.key)
            .bind(&id_str)
            .bind(&document.name)
            .bind(&document.username)
            .bind(&document.password_hash)
            .bind(&document.pin_hash)
            .bind(document.role.as_str())
            .bind(&document.employee_key)
            .bind(if document.is_active { 1i64 } else { 0i64 })
            .bind(&created_str)
            .bind(&updated_str)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id_str).ok();
            Ok(document)
        }
    }
}
