// The wire record for a user in sync (device <-> cloud). Unlike every REST DTO
// it carries the Argon2id `passwordHash`: a cashier created on one device has to
// be able to log in on every other device and on the cloud web app, and the
// hash is the only credential material that can travel. It is meant ONLY for
// the authenticated sync transport (device tokens on the cloud, the shell's
// service token on a device); it must never be returned by a REST endpoint,
// logged, or copied into a conflict record (see `sync::resources::secret_fields`).

use chrono::{DateTime, Utc};
use mongodb::bson::{Bson, Document, oid::ObjectId};
use serde::Serialize;

use crate::core::error::{AppError, AppResult};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserSyncPayload {
    /// Legacy hex id: preserved on every replica because login tokens carry it.
    pub id: String,
    pub key: String,
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub is_active: bool,
    pub employee_key: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub deleted_at: Option<DateTime<Utc>>,
}

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

fn version(doc: &Document) -> i64 {
    match doc.get("version") {
        Some(Bson::Int64(v)) => *v,
        Some(Bson::Int32(v)) => i64::from(*v),
        _ => 1,
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
                email: field("email")?,
                password_hash: field("password_hash")?,
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
