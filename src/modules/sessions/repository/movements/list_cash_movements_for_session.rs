use super::cash_movements_col::cash_movements_col;
use super::movement_from_sqlite_row::movement_from_sqlite_row;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::sessions::model::CashMovementDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

pub async fn list_cash_movements_for_session(
    db: &Db,
    session_key: &str,
) -> AppResult<Vec<CashMovementDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let mut cursor = cash_movements_col(mongo_db)
                .find(doc! { "session_key": session_key })
                .sort(doc! { "created_at": 1 })
                .await?;
            let mut results = Vec::new();
            while let Some(item) = cursor.try_next().await? {
                results.push(item);
            }
            Ok(results)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM cash_movements WHERE session_key = ? AND deleted_at IS NULL ORDER BY created_at ASC"
            )
            .bind(session_key)
            .fetch_all(pool)
            .await?;

            let mut results = Vec::with_capacity(rows.len());
            for r in rows {
                results.push(movement_from_sqlite_row(&r)?);
            }
            Ok(results)
        }
    }
}
