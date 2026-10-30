use super::user_from_sqlite_row::user_from_sqlite_row;
use super::users::users;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::users::model::UserDocument;
use futures_util::TryStreamExt;

use mongodb::bson::{Bson, Document, doc};

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
