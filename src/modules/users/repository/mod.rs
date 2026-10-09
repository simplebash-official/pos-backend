// Database access for the `users` collection/table. Functions here never
// interpret a missing document as an error — they return `Option`/`Vec`
// straight from the driver and leave the "not found" -> `AppError`
// translation to `service`.

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{Bson, DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{generate_id_hex, to_bson_datetime},
    },
    core::error::AppResult,
    domain::users::Role,
    modules::users::model::UserDocument,
};

fn users(db: &TenantDatabase) -> ScopedCollection<UserDocument> {
    db.collection("users")
}

fn user_from_sqlite_row(r: &sqlx::sqlite::SqliteRow) -> UserDocument {
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

pub(crate) async fn find_user_by_id(db: &Db, id: ObjectId) -> AppResult<Option<UserDocument>> {
    match db {
        // `deleted_at: null` matches live users: a user deleted through sync (or
        // the REST delete) is a tombstone on Mongo and must not be found again.
        Db::Mongo(db) => Ok(users(db)
            .find_one(doc! { "_id": id, "deleted_at": Bson::Null })
            .await?),
        Db::Sqlite(pool) => {
            let id_hex = id.to_hex();
            let row = sqlx::query("SELECT * FROM users WHERE id = $1")
                .bind(&id_hex)
                .fetch_optional(pool)
                .await?;
            Ok(row.as_ref().map(user_from_sqlite_row))
        }
    }
}

/// Looked up by normalized (lowercase) username — the entry point both
/// `create_user`'s uniqueness check and `verify_credentials`'s login lookup
/// use. Scoped to the current shop by the tenant-aware collection.
pub(crate) async fn find_user_by_username(
    db: &Db,
    username: &str,
) -> AppResult<Option<UserDocument>> {
    match db {
        Db::Mongo(db) => Ok(users(db)
            .find_one(doc! { "username": username, "deleted_at": Bson::Null })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM users WHERE LOWER(username) = LOWER($1)")
                .bind(username)
                .fetch_optional(pool)
                .await?;
            Ok(row.as_ref().map(user_from_sqlite_row))
        }
    }
}

/// Looked up by linked employee key — backs `service::find_user_by_employee_key`,
/// the entry point `modules::employees` uses to answer "does this employee
/// already have a login" (on create-validation and on delete's
/// `EMPLOYEE_HAS_LOGIN` guard).
pub(crate) async fn find_user_by_employee_key(
    db: &Db,
    employee_key: &str,
) -> AppResult<Option<UserDocument>> {
    match db {
        Db::Mongo(db) => Ok(users(db)
            .find_one(doc! { "employee_key": employee_key, "deleted_at": Bson::Null })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM users WHERE employee_key = $1")
                .bind(employee_key)
                .fetch_optional(pool)
                .await?;
            Ok(row.as_ref().map(user_from_sqlite_row))
        }
    }
}

/// Batch variant of `find_user_by_employee_key` — used by
/// `service::find_user_summaries_by_employee_keys` to enrich a whole page of
/// employees with their login summary in one query instead of one lookup
/// per row.
pub(crate) async fn find_users_by_employee_keys(
    db: &Db,
    employee_keys: &[String],
) -> AppResult<Vec<UserDocument>> {
    if employee_keys.is_empty() {
        return Ok(Vec::new());
    }

    match db {
        Db::Mongo(db) => {
            let mut cursor = users(db)
                .find(doc! { "employee_key": { "$in": employee_keys }, "deleted_at": Bson::Null })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let placeholders: Vec<String> =
                (1..=employee_keys.len()).map(|i| format!("${i}")).collect();
            let query_str = format!(
                "SELECT * FROM users WHERE employee_key IN ({})",
                placeholders.join(", ")
            );
            let mut query = sqlx::query(&query_str);
            for k in employee_keys {
                query = query.bind(k);
            }
            let rows = query.fetch_all(pool).await?;
            Ok(rows.iter().map(user_from_sqlite_row).collect())
        }
    }
}

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
                INSERT INTO users (key, id, name, username, password_hash, role, employee_key, is_active, version, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 1, $9, $10)
                "#,
            )
            .bind(&document.key)
            .bind(&id_str)
            .bind(&document.name)
            .bind(&document.username)
            .bind(&document.password_hash)
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

pub(crate) async fn delete_user(db: &Db, id: ObjectId) -> AppResult<Option<UserDocument>> {
    match db {
        // A tombstone, not a removal: the cloud change stream only sees updates,
        // so this is what carries the delete to every device. Returns the user as
        // it was before deletion, like the SQLite arm.
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            Ok(users(db)
                .find_one_and_update(
                    doc! { "_id": id, "deleted_at": Bson::Null },
                    doc! { "$set": { "deleted_at": now, "updated_at": now } },
                )
                .return_document(ReturnDocument::Before)
                .await?)
        }
        Db::Sqlite(pool) => {
            let user = find_user_by_id(db, id).await?;
            if user.is_some() {
                let id_hex = id.to_hex();
                sqlx::query("DELETE FROM users WHERE id = $1")
                    .bind(&id_hex)
                    .execute(pool)
                    .await?;
            }
            Ok(user)
        }
    }
}

/// Sorted by name for stable, predictable `GET /users` output. `filter` is
/// fully assembled by the caller (`service::list_users`).
pub(crate) async fn list_users(db: &Db, filter: Document) -> AppResult<Vec<UserDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut filter = filter;
            filter.insert("deleted_at", Bson::Null);
            let mut cursor = users(db).find(filter).sort(doc! { "name": 1 }).await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            // Check for role filter or is_active filter
            let role_filter = filter.get_document("role").ok().and_then(|r| {
                r.get_array("$in").ok().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<String>>()
                })
            });

            let rows = if let Some(roles) = role_filter {
                if roles.is_empty() {
                    return Ok(Vec::new());
                }
                let placeholders: Vec<String> =
                    (1..=roles.len()).map(|i| format!("${i}")).collect();
                let query_str = format!(
                    "SELECT * FROM users WHERE role IN ({}) ORDER BY name ASC",
                    placeholders.join(", ")
                );
                let mut query = sqlx::query(&query_str);
                for r in roles {
                    query = query.bind(r);
                }
                query.fetch_all(pool).await?
            } else {
                sqlx::query("SELECT * FROM users ORDER BY name ASC")
                    .fetch_all(pool)
                    .await?
            };

            Ok(rows.iter().map(user_from_sqlite_row).collect())
        }
    }
}

/// Backs the single-Admin invariant in `service::create_user` — this is a
/// single-shop POS deployment, so there should never be more than one
/// Admin account.
pub(crate) async fn count_users_by_role(db: &Db, role: Role) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(users(db)
            .count_documents(doc! { "role": role.as_str(), "deleted_at": Bson::Null })
            .await?),
        Db::Sqlite(pool) => {
            let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE role = $1")
                .bind(role.as_str())
                .fetch_one(pool)
                .await?;
            Ok(row.0 as u64)
        }
    }
}
