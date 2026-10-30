use super::cash_movements_col::cash_movements_col;
use super::movement_from_sqlite_row::movement_from_sqlite_row;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::sessions::model::CashMovementDocument;

use mongodb::bson::doc;

pub async fn find_cash_movement_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<CashMovementDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let doc = cash_movements_col(mongo_db)
                .find_one(doc! { "key": key })
                .await?;
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let row =
                sqlx::query("SELECT * FROM cash_movements WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;

            row.map(|r| movement_from_sqlite_row(&r)).transpose()
        }
    }
}
