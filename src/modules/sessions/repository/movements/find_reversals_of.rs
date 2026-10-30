use super::cash_movements_col::cash_movements_col;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use futures_util::TryStreamExt;
use mongodb::bson::doc;

use sqlx::Row;

/// For each of the given movement keys, the key of the reversal that cancels it, if one exists.
pub async fn find_reversals_of(db: &Db, keys: &[String]) -> AppResult<Vec<(String, String)>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(mongo_db) => {
            let mut cursor = cash_movements_col(mongo_db)
                .find(doc! { "reverses_key": { "$in": keys } })
                .await?;
            let mut out = Vec::new();
            while let Some(item) = cursor.try_next().await? {
                if let Some(original) = item.reverses_key {
                    out.push((original, item.key));
                }
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let placeholders = keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT reverses_key, key FROM cash_movements WHERE reverses_key IN ({placeholders}) AND deleted_at IS NULL"
            );
            let mut query = sqlx::query(&sql);
            for key in keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            Ok(rows
                .iter()
                .map(|r| (r.get("reverses_key"), r.get("key")))
                .collect())
        }
    }
}
