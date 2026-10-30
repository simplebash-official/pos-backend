use super::find_user_by_id::find_user_by_id;
use super::users::users;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::users::model::UserDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::ReturnDocument;

/// `set_doc` is built by the caller (`service::update_user`), which decides
/// which fields actually change — this just applies it.
pub(crate) async fn update_user(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<UserDocument>> {
    match db {
        Db::Mongo(db) => Ok(users(db)
            .find_one_and_update(
                doc! { "_id": id, "deleted_at": Bson::Null },
                doc! { "$set": set_doc },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_hex = id.to_hex();
            let mut updates = Vec::new();
            let mut params: Vec<String> = Vec::new();

            if let Ok(name) = set_doc.get_str("name") {
                updates.push(format!("name = ${}", updates.len() + 1));
                params.push(name.to_string());
            }
            if let Ok(username) = set_doc.get_str("username") {
                updates.push(format!("username = ${}", updates.len() + 1));
                params.push(username.to_string());
            }
            if let Ok(password_hash) = set_doc.get_str("password_hash") {
                updates.push(format!("password_hash = ${}", updates.len() + 1));
                params.push(password_hash.to_string());
            }
            if let Ok(pin_hash) = set_doc.get_str("pin_hash") {
                updates.push(format!("pin_hash = ${}", updates.len() + 1));
                params.push(pin_hash.to_string());
            }
            if let Ok(role) = set_doc.get_str("role") {
                updates.push(format!("role = ${}", updates.len() + 1));
                params.push(role.to_string());
            }
            if let Ok(is_active) = set_doc.get_bool("is_active") {
                updates.push(format!("is_active = ${}", updates.len() + 1));
                params.push(if is_active {
                    "1".to_string()
                } else {
                    "0".to_string()
                });
            }
            if let Ok(emp_key) = set_doc.get_str("employee_key") {
                updates.push(format!("employee_key = ${}", updates.len() + 1));
                params.push(emp_key.to_string());
            }

            if let Ok(preferences) = set_doc.get_document("preferences")
                && let Ok(json) = serde_json::to_string(preferences)
            {
                updates.push(format!("preferences_json = ${}", updates.len() + 1));
                params.push(json);
            }

            let now_str = crate::clients::sqlite::now_utc_iso();
            updates.push(format!("updated_at = ${}", updates.len() + 1));
            params.push(now_str);

            if !updates.is_empty() {
                let id_idx = updates.len() + 1;
                let sql = format!(
                    "UPDATE users SET {} WHERE id = ${id_idx}",
                    updates.join(", ")
                );
                let mut q = sqlx::query(&sql);
                for p in params {
                    q = q.bind(p);
                }
                q = q.bind(&id_hex);
                q.execute(pool).await?;
            }

            find_user_by_id(db, id).await
        }
    }
}
