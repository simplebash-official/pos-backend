use super::login_sessions::login_sessions;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::auth::model::LoginSessionDocument;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert_login_session(
    db: &Db,
    mut document: LoginSessionDocument,
) -> AppResult<LoginSessionDocument> {
    match db {
        Db::Mongo(db) => {
            let result = login_sessions(db).insert_one(&document).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let created_at = bson_to_iso(&document.created_at);
            let updated_at = bson_to_iso(&document.updated_at);

            sqlx::query(
                r#"
                INSERT INTO login_sessions (
                    id, key, user_key, name_at_login, username_at_login, role_at_login,
                    ip_address, user_agent, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&document.key)
            .bind(&document.user_key)
            .bind(&document.name_at_login)
            .bind(&document.username_at_login)
            .bind(document.role_at_login.as_str())
            .bind(&document.ip_address)
            .bind(&document.user_agent)
            .bind(&created_at)
            .bind(&updated_at)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
