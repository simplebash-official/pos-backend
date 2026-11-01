use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, now_utc_iso, to_bson_datetime};
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;
use crate::modules::sessions::model::DrawerStateDocument;
use mongodb::bson::{DateTime as BsonDateTime, doc};
use sqlx::Row;

pub(super) fn drawer_state_col(db: &TenantDatabase) -> ScopedCollection<DrawerStateDocument> {
    db.collection("drawer_state")
}

/// Returns the drawer state.
pub async fn get_drawer_state(db: &Db, user_key: &str) -> AppResult<Option<DrawerStateDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let doc = drawer_state_col(mongo_db)
                .find_one(doc! { "user_key": user_key })
                .await?;
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT user_key, current_balance_cents, last_session_key, last_closing_float_cents, updated_at FROM drawer_state WHERE user_key = ?"
            )
            .bind(user_key)
            .fetch_optional(pool)
            .await?;

            row.map(|r| -> AppResult<DrawerStateDocument> {
                let updated_at_str: String = r.get("updated_at");
                Ok(DrawerStateDocument {
                    user_key: r.get("user_key"),
                    current_balance_cents: r.get("current_balance_cents"),
                    last_session_key: r.get("last_session_key"),
                    last_closing_float_cents: r.get("last_closing_float_cents"),
                    updated_at: to_bson_datetime(&updated_at_str)?,
                })
            })
            .transpose()
        }
    }
}

/// The drawer balance of every user, for the shop-wide cash total.
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
                "SELECT user_key, current_balance_cents, last_session_key, last_closing_float_cents, updated_at FROM drawer_state",
            )
            .fetch_all(pool)
            .await?;
            rows.iter()
                .map(|r| -> AppResult<DrawerStateDocument> {
                    Ok(DrawerStateDocument {
                        user_key: r.get("user_key"),
                        current_balance_cents: r.get("current_balance_cents"),
                        last_session_key: r.get("last_session_key"),
                        last_closing_float_cents: r.get("last_closing_float_cents"),
                        updated_at: to_bson_datetime(&r.get::<String, _>("updated_at"))?,
                    })
                })
                .collect::<AppResult<Vec<_>>>()
        }
    }
}

/// Updates the drawer balance.
pub async fn update_drawer_balance(
    unit: impl Into<Unit<'_>>,
    user_key: &str,
    delta_cents: i64,
) -> AppResult<()> {
    let now = BsonDateTime::now();
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(mongo_db, session) => {
            crate::in_session!(
                drawer_state_col(mongo_db)
                    .find_one_and_update(
                        doc! { "user_key": user_key },
                        doc! {
                            "$inc": { "current_balance_cents": delta_cents },
                            "$set": { "updated_at": now },
                            // A first-ever movement for a user creates the row; it must be a complete drawer state.
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
                INSERT INTO drawer_state (user_key, current_balance_cents, last_session_key, last_closing_float_cents, updated_at)
                VALUES (?, ?, NULL, 0, ?)
                ON CONFLICT(user_key) DO UPDATE SET
                    current_balance_cents = current_balance_cents + excluded.current_balance_cents,
                    updated_at = excluded.updated_at
                "#
            )
            .bind(user_key)
            .bind(delta_cents)
            .bind(&now_iso)
            .execute(&mut *conn)
            .await?;
            Ok(())
        }
    }
}

/// Inserts or updates the drawer state.
pub async fn upsert_drawer_state(
    db: &Db,
    state: DrawerStateDocument,
) -> AppResult<DrawerStateDocument> {
    match db {
        Db::Mongo(mongo_db) => {
            let col = drawer_state_col(mongo_db);
            let filter = doc! { "user_key": &state.user_key };
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
                INSERT INTO drawer_state (user_key, current_balance_cents, last_session_key, last_closing_float_cents, updated_at)
                VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(user_key) DO UPDATE SET
                    current_balance_cents = excluded.current_balance_cents,
                    last_session_key = excluded.last_session_key,
                    last_closing_float_cents = excluded.last_closing_float_cents,
                    updated_at = excluded.updated_at
                "#
            )
            .bind(&state.user_key)
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
