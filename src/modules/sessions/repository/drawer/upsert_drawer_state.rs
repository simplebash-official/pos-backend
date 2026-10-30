use super::drawer_state_col::drawer_state_col;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;
use crate::modules::sessions::model::DrawerStateDocument;
use mongodb::bson::doc;

pub async fn upsert_drawer_state(
    db: &Db,
    state: DrawerStateDocument,
) -> AppResult<DrawerStateDocument> {
    match db {
        Db::Mongo(mongo_db) => {
            let col = drawer_state_col(mongo_db);
            let filter = doc! { "terminal_id": &state.terminal_id };
            if col.find_one(filter.clone()).await?.is_some() {
                col.replace_one(filter, &state).await?;
            } else {
                col.insert_one(&state).await?;
            }
            Ok(state)
        }
        Db::Sqlite(pool) => {
            let updated_at_iso = bson_to_iso(&state.updated_at);
            sqlx::query(
                r#"
                INSERT INTO drawer_state (terminal_id, current_balance_cents, last_session_key, last_closing_float_cents, updated_at)
                VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(terminal_id) DO UPDATE SET
                    current_balance_cents = excluded.current_balance_cents,
                    last_session_key = excluded.last_session_key,
                    last_closing_float_cents = excluded.last_closing_float_cents,
                    updated_at = excluded.updated_at
                "#
            )
            .bind(&state.terminal_id)
            .bind(state.current_balance_cents)
            .bind(&state.last_session_key)
            .bind(state.last_closing_float_cents)
            .bind(&updated_at_iso)
            .execute(pool)
            .await?;

            Ok(state)
        }
    }
}
