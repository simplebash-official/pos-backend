use super::session_from_sqlite_row::session_from_sqlite_row;
use super::sessions_col::sessions_col;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::sessions::model::RegisterSessionDocument;

use mongodb::bson::doc;

pub async fn find_active_session_for_cashier(
    db: &Db,
    cashier_id: &str,
) -> AppResult<Option<RegisterSessionDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let doc = sessions_col(mongo_db)
                .find_one(doc! {
                    "cashier_id": cashier_id,
                    "status": { "$in": ["open", "closing", "reopened"] }
                })
                .await?;
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM register_sessions WHERE cashier_id = ? AND status IN ('open', 'closing', 'reopened') AND deleted_at IS NULL"
            )
            .bind(cashier_id)
            .fetch_optional(pool)
            .await?;

            row.map(|r| session_from_sqlite_row(&r)).transpose()
        }
    }
}
