use super::drawer_state_col::drawer_state_col;
use crate::clients::db::Db;
use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::modules::sessions::model::DrawerStateDocument;
use mongodb::bson::doc;
use sqlx::Row;

/// The drawer balance of every terminal, for the shop-wide cash total.
pub async fn list_drawer_states(db: &Db) -> AppResult<Vec<DrawerStateDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let mut cursor = drawer_state_col(mongo_db).find(doc! {}).await?;
            let mut out = Vec::new();
            while let Some(d) = futures_util::TryStreamExt::try_next(&mut cursor).await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT terminal_id, current_balance_cents, last_session_key, last_closing_float_cents, updated_at FROM drawer_state",
            )
            .fetch_all(pool)
            .await?;
            Ok(rows
                .iter()
                .map(|r| DrawerStateDocument {
                    terminal_id: r.get("terminal_id"),
                    current_balance_cents: r.get("current_balance_cents"),
                    last_session_key: r.get("last_session_key"),
                    last_closing_float_cents: r.get("last_closing_float_cents"),
                    updated_at: to_bson_datetime(&r.get::<String, _>("updated_at")),
                })
                .collect())
        }
    }
}
