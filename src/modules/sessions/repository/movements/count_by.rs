use super::cash_movements_col::cash_movements_col;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use futures_util::TryStreamExt;
use mongodb::bson::doc;

use sqlx::Row;

pub(super) async fn count_by(db: &Db, field: &'static str) -> AppResult<Vec<(String, i64)>> {
    match db {
        Db::Mongo(mongo_db) => {
            let pipeline = vec![
                doc! { "$match": { field: { "$type": "string" } } },
                doc! { "$group": { "_id": format!("${field}"), "n": { "$sum": 1 } } },
            ];
            let mut cursor = cash_movements_col(mongo_db).aggregate(pipeline).await?;
            let mut out = Vec::new();
            while let Some(row) = cursor.try_next().await? {
                let key = row
                    .get_str("_id")
                    .map_err(|e| AppError::internal(e.to_string()))?;
                let n = match row.get("n") {
                    Some(mongodb::bson::Bson::Int32(n)) => i64::from(*n),
                    Some(mongodb::bson::Bson::Int64(n)) => *n,
                    _ => 0,
                };
                out.push((key.to_string(), n));
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let sql = format!(
                "SELECT {field} AS k, COUNT(*) AS n FROM cash_movements \
                 WHERE {field} IS NOT NULL AND deleted_at IS NULL GROUP BY {field}"
            );
            let rows = sqlx::query(&sql).fetch_all(pool).await?;
            Ok(rows.iter().map(|r| (r.get("k"), r.get("n"))).collect())
        }
    }
}
