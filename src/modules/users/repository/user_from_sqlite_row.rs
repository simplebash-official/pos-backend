use crate::clients::sqlite::to_bson_datetime;

use crate::domain::users::Role;
use crate::modules::users::model::UserDocument;

use mongodb::bson::oid::ObjectId;

use sqlx::Row;

pub(super) fn user_from_sqlite_row(r: &sqlx::sqlite::SqliteRow) -> UserDocument {
    let id_str: String = r.get("id");
    let role_str: String = r.get("role");
    let created_str: String = r.get("created_at");
    let updated_str: String = r.get("updated_at");
    let is_active_int: i64 = r.get("is_active");
    UserDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: r.get("key"),
        name: r.get("name"),
        username: r.get("username"),
        password_hash: r.get("password_hash"),
        pin_hash: r.try_get::<Option<String>, _>("pin_hash").ok().flatten(),
        role: Role::from_str(&role_str).unwrap_or(Role::Staff),
        is_active: is_active_int != 0,
        employee_key: r.get("employee_key"),
        preferences: r
            .try_get::<Option<String>, _>("preferences_json")
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str(&raw).ok()),
        created_at: to_bson_datetime(&created_str),
        updated_at: to_bson_datetime(&updated_str),
    }
}
