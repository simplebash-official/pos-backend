use super::drawer_state_col::drawer_state_col;

use crate::clients::sqlite::now_utc_iso;

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;

use mongodb::bson::{DateTime as BsonDateTime, doc};

pub async fn update_drawer_balance(
    unit: impl Into<Unit<'_>>,
    terminal_id: &str,
    delta_cents: i64,
) -> AppResult<()> {
    let now = BsonDateTime::now();
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(mongo_db, session) => {
            crate::in_session!(
                drawer_state_col(mongo_db)
                    .find_one_and_update(
                        doc! { "terminal_id": terminal_id },
                        doc! {
                            "$inc": { "current_balance_cents": delta_cents },
                            "$set": { "updated_at": now },
                            // A first-ever movement on a terminal creates the row; it must be a complete drawer state.
                            "$setOnInsert": { "last_closing_float_cents": 0_i64 },
                        },
                    )
                    .upsert(true),
                session
            )?;
            Ok(())
        }
        Backend::Sqlite(mut conn) => {
            let now_iso = now_utc_iso();
            sqlx::query(
                r#"
                INSERT INTO drawer_state (terminal_id, current_balance_cents, last_session_key, last_closing_float_cents, updated_at)
                VALUES (?, ?, NULL, 0, ?)
                ON CONFLICT(terminal_id) DO UPDATE SET
                    current_balance_cents = current_balance_cents + excluded.current_balance_cents,
                    updated_at = excluded.updated_at
                "#
            )
            .bind(terminal_id)
            .bind(delta_cents)
            .bind(&now_iso)
            .execute(&mut *conn)
            .await?;
            Ok(())
        }
    }
}
