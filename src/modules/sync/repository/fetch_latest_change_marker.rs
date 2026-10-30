use crate::clients::db::Db;

use crate::clients::tenant_db::ScopedCollection;
use crate::core::error::AppResult;

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use sqlx::Row;

/// The newest `(updated_at, key)` pair in a collection, which is what a
/// cursor has to be built from. Sorted the same way `fetch_collection_changes`
/// pages, only descending, so the pair is exactly the point a delta pull
/// would resume from after consuming every existing row.
pub(crate) async fn fetch_latest_change_marker(
    db: &Db,
    collection_name: &str,
) -> AppResult<Option<(BsonDateTime, String)>> {
    match db {
        Db::Mongo(db) => {
            let collection: ScopedCollection<Document> = db.collection(collection_name);
            let doc = collection
                .find_one(doc! { "updated_at": { "$exists": true } })
                .sort(doc! { "updated_at": -1, "key": -1 })
                .await?;

            Ok(doc.and_then(|d| {
                let updated_at = d.get_datetime("updated_at").copied().ok()?;
                let key = d.get_str("key").unwrap_or_default().to_string();
                Some((updated_at, key))
            }))
        }
        Db::Sqlite(pool) => {
            let table = collection_name;
            let query_str = format!(
                "SELECT updated_at, key FROM {table} ORDER BY updated_at DESC, key DESC LIMIT 1"
            );
            let row = sqlx::query(&query_str).fetch_optional(pool).await?;

            Ok(row.and_then(|r| {
                let updated_at_str: String = r.get("updated_at");
                let key: String = r.get("key");
                let dt = chrono::DateTime::parse_from_rfc3339(&updated_at_str).ok()?;
                Some((
                    BsonDateTime::from_chrono(dt.with_timezone(&chrono::Utc)),
                    key,
                ))
            }))
        }
    }
}
