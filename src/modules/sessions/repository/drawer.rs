// Persistent drawer balance ledger operations for SQLite and MongoDB.

use mongodb::bson::{DateTime as BsonDateTime, doc};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, now_utc_iso, to_bson_datetime},
        tenant_db::{ScopedCollection, TenantDatabase},
        uow::{Backend, Unit},
    },
    core::error::AppResult,
    modules::sessions::model::DrawerStateDocument,
};

fn drawer_state_col(db: &TenantDatabase) -> ScopedCollection<DrawerStateDocument> {
    db.collection("drawer_state")
}

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
