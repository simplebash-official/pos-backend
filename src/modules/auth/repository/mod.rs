// Dual-engine (Mongo / SQLite) access for the `login_sessions` audit log.
// Visibility is `pub(crate)` so `service` can call in, but `mod repository;`
// is private to `modules/auth`.

use futures_util::TryStreamExt;
use mongodb::{
    Collection,
    bson::{Document, doc, oid::ObjectId},
};

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
    },
    core::error::AppResult,
    domain::users::Role,
    modules::auth::model::LoginSessionDocument,
};

#[derive(sqlx::FromRow)]
struct LoginSessionSqliteRow {
    id: String,
    key: String,
    user_key: String,
    name_at_login: String,
    email_at_login: String,
    role_at_login: String,
    ip_address: Option<String>,
    user_agent: Option<String>,
    created_at: String,
    updated_at: String,
}

impl LoginSessionSqliteRow {
    fn into_document(self) -> LoginSessionDocument {
        LoginSessionDocument {
            id: ObjectId::parse_str(&self.id).ok(),
            key: self.key,
            user_key: self.user_key,
            name_at_login: self.name_at_login,
            email_at_login: self.email_at_login,
            role_at_login: self.role_at_login.parse().unwrap_or(Role::Staff),
            ip_address: self.ip_address,
            user_agent: self.user_agent,
            created_at: to_bson_datetime(&self.created_at),
            updated_at: to_bson_datetime(&self.updated_at),
        }
    }
}

fn login_sessions(db: &mongodb::Database) -> Collection<LoginSessionDocument> {
    db.collection("login_sessions")
}

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
                    id, key, user_key, name_at_login, email_at_login, role_at_login,
                    ip_address, user_agent, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&document.key)
            .bind(&document.user_key)
            .bind(&document.name_at_login)
            .bind(&document.email_at_login)
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

pub(crate) async fn list_login_sessions(
    db: &Db,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<Vec<LoginSessionDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = login_sessions(db)
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .skip(skip)
                .limit(limit as i64)
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let user_key = filter.get_str("user_key").ok();
            let rows: Vec<LoginSessionSqliteRow> = match user_key {
                Some(uk) => {
                    sqlx::query_as(
                        r#"
                        SELECT id, key, user_key, name_at_login, email_at_login, role_at_login,
                               ip_address, user_agent, created_at, updated_at
                        FROM login_sessions
                        WHERE user_key = ?
                        ORDER BY created_at DESC
                        LIMIT ? OFFSET ?
                        "#,
                    )
                    .bind(uk)
                    .bind(limit as i64)
                    .bind(skip as i64)
                    .fetch_all(pool)
                    .await?
                }
                None => {
                    sqlx::query_as(
                        r#"
                        SELECT id, key, user_key, name_at_login, email_at_login, role_at_login,
                               ip_address, user_agent, created_at, updated_at
                        FROM login_sessions
                        ORDER BY created_at DESC
                        LIMIT ? OFFSET ?
                        "#,
                    )
                    .bind(limit as i64)
                    .bind(skip as i64)
                    .fetch_all(pool)
                    .await?
                }
            };

            Ok(rows.into_iter().map(|r| r.into_document()).collect())
        }
    }
}

pub(crate) async fn count_login_sessions(db: &Db, filter: Document) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(login_sessions(db).count_documents(filter).await?),
        Db::Sqlite(pool) => {
            let user_key = filter.get_str("user_key").ok();
            let count: i64 = match user_key {
                Some(uk) => {
                    sqlx::query_scalar("SELECT COUNT(*) FROM login_sessions WHERE user_key = ?")
                        .bind(uk)
                        .fetch_one(pool)
                        .await?
                }
                None => {
                    sqlx::query_scalar("SELECT COUNT(*) FROM login_sessions")
                        .fetch_one(pool)
                        .await?
                }
            };
            Ok(count as u64)
        }
    }
}
