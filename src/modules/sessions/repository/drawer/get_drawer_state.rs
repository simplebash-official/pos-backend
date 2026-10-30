use super::drawer_state_col::drawer_state_col;
use crate::clients::db::Db;
use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::modules::sessions::model::DrawerStateDocument;
use mongodb::bson::doc;
use sqlx::Row;

pub async fn get_drawer_state(
    db: &Db,
    terminal_id: &str,
) -> AppResult<Option<DrawerStateDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let doc = drawer_state_col(mongo_db)
                .find_one(doc! { "terminal_id": terminal_id })
                .await?;
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT terminal_id, current_balance_cents, last_session_key, last_closing_float_cents, updated_at FROM drawer_state WHERE terminal_id = ?"
            )
            .bind(terminal_id)
            .fetch_optional(pool)
            .await?;

            Ok(row.map(|r| {
                let updated_at_str: String = r.get("updated_at");
                DrawerStateDocument {
                    terminal_id: r.get("terminal_id"),
                    current_balance_cents: r.get("current_balance_cents"),
                    last_session_key: r.get("last_session_key"),
                    last_closing_float_cents: r.get("last_closing_float_cents"),
                    updated_at: to_bson_datetime(&updated_at_str),
                }
            }))
        }
    }
}
