use super::login_sessions::login_sessions;
use crate::clients::db::Db;
use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::AppResult;
use crate::domain::users::Role;
use crate::modules::auth::model::LoginSessionDocument;
use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Document, doc};

#[derive(Debug, sqlx::FromRow)]
struct LoginSessionSqliteRow {
    id: String,
    key: String,
    user_key: String,
    name_at_login: String,
    username_at_login: String,
    role_at_login: String,
    ip_address: Option<String>,
    user_agent: Option<String>,
    created_at: String,
    updated_at: String,
}

impl LoginSessionSqliteRow {
    pub(super) fn into_document(self) -> LoginSessionDocument {
        LoginSessionDocument {
            id: ObjectId::parse_str(&self.id).ok(),
            key: self.key,
            user_key: self.user_key,
            name_at_login: self.name_at_login,
            username_at_login: self.username_at_login,
            role_at_login: self.role_at_login.parse().unwrap_or(Role::Staff),
            ip_address: self.ip_address,
            user_agent: self.user_agent,
            created_at: to_bson_datetime(&self.created_at),
            updated_at: to_bson_datetime(&self.updated_at),
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
                        SELECT id, key, user_key, name_at_login, username_at_login, role_at_login,
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
                        SELECT id, key, user_key, name_at_login, username_at_login, role_at_login,
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
