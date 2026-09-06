use crate::{
    clients::{db::Db, sqlite::map_sqlite_row_to_document},
    core::error::AppResult,
    modules::sync::cursor::DecodedCursor,
};
use futures_util::TryStreamExt;
use mongodb::{
    Collection,
    bson::{DateTime as BsonDateTime, Document, doc},
};
use sqlx::Row;

pub(crate) async fn fetch_collection_changes(
    db: &Db,
    collection_name: &str,
    cursor: Option<&DecodedCursor>,
    limit: i64,
) -> AppResult<Vec<Document>> {
    match db {
        Db::Mongo(db) => {
            let collection: Collection<Document> = db.collection(collection_name);

            let filter = match cursor {
                // A snapshot must not carry tombstones: the client has no row to
                // delete yet, so a deleted doc would just be noise. Deltas keep
                // them, since that is the only way a peer learns about a delete.
                None => doc! { "deleted_at": { "$exists": false } },
                Some(c) => {
                    let bson_time = BsonDateTime::from_chrono(c.timestamp);
                    if let Some(ref k) = c.key {
                        doc! {
                            "$or": [
                                { "updated_at": { "$gt": bson_time } },
                                { "updated_at": bson_time, "key": { "$gt": k } }
                            ]
                        }
                    } else {
                        doc! { "updated_at": { "$gt": bson_time } }
                    }
                }
            };

            let mut db_cursor = collection
                .find(filter)
                .sort(doc! { "updated_at": 1, "key": 1 })
                .limit(limit)
                .await?;

            let mut items = Vec::new();
            while let Some(doc) = db_cursor.try_next().await? {
                items.push(doc);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let table = collection_name;
            let has_deleted_at = matches!(
                table,
                "products" | "categories" | "customers" | "suppliers" | "employees"
            );

            let rows = match cursor {
                None => {
                    let query_str = if has_deleted_at {
                        format!(
                            "SELECT * FROM {table} WHERE deleted_at IS NULL ORDER BY updated_at ASC, key ASC LIMIT ?"
                        )
                    } else {
                        format!("SELECT * FROM {table} ORDER BY updated_at ASC, key ASC LIMIT ?")
                    };
                    sqlx::query(&query_str).bind(limit).fetch_all(pool).await?
                }
                Some(c) => {
                    let ts_iso = c.timestamp.to_rfc3339();
                    if let Some(ref k) = c.key {
                        let query_str = format!(
                            "SELECT * FROM {table} WHERE (updated_at > ? OR (updated_at = ? AND key > ?)) ORDER BY updated_at ASC, key ASC LIMIT ?"
                        );
                        sqlx::query(&query_str)
                            .bind(&ts_iso)
                            .bind(&ts_iso)
                            .bind(k)
                            .bind(limit)
                            .fetch_all(pool)
                            .await?
                    } else {
                        let query_str = format!(
                            "SELECT * FROM {table} WHERE updated_at > ? ORDER BY updated_at ASC, key ASC LIMIT ?"
                        );
                        sqlx::query(&query_str)
                            .bind(&ts_iso)
                            .bind(limit)
                            .fetch_all(pool)
                            .await?
                    }
                }
            };

            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(map_sqlite_row_to_document(&row));
            }
            Ok(items)
        }
    }
}

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
            let collection: Collection<Document> = db.collection(collection_name);
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
