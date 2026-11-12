use crate::core::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, Document};
use serde::Serialize;

fn text(doc: &Document, field: &str) -> Option<String> {
    doc.get_str(field).ok().map(str::to_owned)
}

fn time(doc: &Document, field: &str) -> Option<DateTime<Utc>> {
    match doc.get(field) {
        Some(Bson::DateTime(t)) => Some(t.to_chrono()),
        Some(Bson::String(s)) => DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| t.with_timezone(&Utc)),
        _ => None,
    }
}

fn flag(doc: &Document, field: &str) -> bool {
    match doc.get(field) {
        Some(Bson::Boolean(b)) => *b,
        Some(Bson::Int32(i)) => *i != 0,
        Some(Bson::Int64(i)) => *i != 0,
        // Absent = the column default (an active account).
        _ => true,
    }
}

/// Raw stored user rows/documents -> sync payloads. Works for both engines: a
/// SQLite row (via `map_sqlite_row_to_document`) and a Mongo `users` document
/// use the same snake_case field names.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<UserSyncPayload>> {
    documents
        .into_iter()
        .map(|doc| {
            let id = doc
                .get_object_id("_id")
                .or_else(|_| doc.get_object_id("id"))
                .map(|oid: ObjectId| oid.to_hex())
                .or_else(|_| doc.get_str("id").map(str::to_owned))
                .map_err(|_| AppError::internal("user document has no id"))?;
            let field = |name: &str| {
                text(&doc, name)
                    .ok_or_else(|| AppError::internal(format!("user document has no `{name}`")))
            };
            Ok(UserSyncPayload {
                id,
                key: field("key")?,
                name: field("name")?,
                username: field("username")?,
                password_hash: field("password_hash")?,
                pin_hash: text(&doc, "pin_hash"),
                role: field("role")?,
                is_active: flag(&doc, "is_active"),
                employee_key: text(&doc, "employee_key"),
                created_at: time(&doc, "created_at"),
                updated_at: time(&doc, "updated_at"),
                version: version(&doc),
                deleted_at: time(&doc, "deleted_at"),
            })
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserSyncPayload {
    /// Legacy hex id: preserved on every replica because login tokens carry it.
    pub id: String,
    pub key: String,
    pub name: String,
    pub username: String,
    pub password_hash: String,
    pub pin_hash: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub employee_key: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub deleted_at: Option<DateTime<Utc>>,
}

pub(super) fn version(doc: &Document) -> i64 {
    match doc.get("version") {
        Some(Bson::Int64(v)) => *v,
        Some(Bson::Int32(v)) => i64::from(*v),
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::hydrate_sync_documents;
    use mongodb::bson::{doc, oid::ObjectId};

    #[test]
    fn hydrate_preserves_pin_hash_alongside_password_hash() {
        let oid = ObjectId::new();
        let out = hydrate_sync_documents(vec![doc! {
            "_id": oid,
            "key": "usr_1",
            "name": "Manager",
            "username": "mgr",
            "password_hash": "$argon2$pw",
            "pin_hash": "$argon2$pin",
            "role": "manager",
            "is_active": true,
            "version": 1_i64,
        }])
        .expect("hydrate succeeds");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].pin_hash.as_deref(), Some("$argon2$pin"));
    }
}
