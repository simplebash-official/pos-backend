use crate::clients::sqlite::bson_to_iso;
use crate::clients::sqlite::generate_id_hex;
use crate::clients::sqlite::now_utc_iso;
use crate::clients::sqlite::to_bson_datetime;
use crate::clients::tenant_db::ScopedCollection;
use crate::clients::tenant_db::TenantDatabase;
use crate::clients::uow::Backend;
use crate::clients::uow::Unit;
use crate::core::error::AppResult;
use crate::modules::sessions::model::DrawerStateDocument;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

const SQLITE_SELECT_COLS: &str = "key, id, user_key, current_balance_cents, last_session_key, \
    last_closing_float_cents, version, created_at, updated_at, updated_by_device";

pub(super) fn drawer_state_col(db: &TenantDatabase) -> ScopedCollection<DrawerStateDocument> {
    db.collection("drawer_state")
}

fn drawer_state_from_row(row: &SqliteRow) -> AppResult<DrawerStateDocument> {
    let id_str: String = row.get("id");
    Ok(DrawerStateDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: row.get("key"),
        user_key: row.get("user_key"),
        current_balance_cents: row.get("current_balance_cents"),
        last_session_key: row.get("last_session_key"),
        last_closing_float_cents: row.get("last_closing_float_cents"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at"))?,
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at"))?,
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Returns the drawer state for `user_key` (`"shop"` in SharedShop mode, or user id in Personal mode).
pub async fn get_drawer_state<'a>(
    unit: impl Into<Unit<'a>>,
    user_key: &str,
) -> AppResult<Option<DrawerStateDocument>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(mongo_db, session) => Ok(crate::in_session!(
            drawer_state_col(mongo_db).find_one(doc! { "user_key": user_key }),
            session
        )?),
        Backend::Sqlite(mut conn) => {
            let query = format!("SELECT {SQLITE_SELECT_COLS} FROM drawer_state WHERE user_key = ?");
            let row = sqlx::query(&query)
                .bind(user_key)
                .fetch_optional(&mut *conn)
                .await?;
            row.as_ref().map(drawer_state_from_row).transpose()
        }
    }
}

/// The drawer balance of every drawer row (`"shop"` and/or per-user), for shop-wide cash checks.
pub async fn list_drawer_states<'a>(
    unit: impl Into<Unit<'a>>,
) -> AppResult<Vec<DrawerStateDocument>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(mongo_db, session) => Ok(drawer_state_col(mongo_db)
            .find_all_in(doc! {}, None, session)
            .await?),
        Backend::Sqlite(mut conn) => {
            let query = format!("SELECT {SQLITE_SELECT_COLS} FROM drawer_state");
            let rows = sqlx::query(&query).fetch_all(&mut *conn).await?;
            rows.iter()
                .map(drawer_state_from_row)
                .collect::<AppResult<Vec<_>>>()
        }
    }
}

/// Updates the drawer balance atomically and increments `version`.
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
                            "$inc": { "current_balance_cents": delta_cents, "version": 1_i64 },
                            "$set": { "updated_at": now },
                            // A first-ever movement for a drawer creates the row; keep every required sync field populated.
                            "$setOnInsert": {
                                "key": user_key,
                                "last_closing_float_cents": 0_i64,
                                "created_at": now,
                            },
                        },
                    )
                    .upsert(true),
                session
            )?;
            Ok(())
        }
        Backend::Sqlite(mut conn) => {
            let now_iso = now_utc_iso();
            let id_hex = generate_id_hex();
            sqlx::query(
                r#"
                INSERT INTO drawer_state (
                    key, id, user_key, current_balance_cents, last_session_key,
                    last_closing_float_cents, version, created_at, updated_at, updated_by_device
                )
                VALUES (?, ?, ?, ?, NULL, 0, 1, ?, ?, NULL)
                ON CONFLICT(user_key) DO UPDATE SET
                    current_balance_cents = current_balance_cents + excluded.current_balance_cents,
                    version = drawer_state.version + 1,
                    updated_at = excluded.updated_at
                "#,
            )
            .bind(user_key)
            .bind(&id_hex)
            .bind(user_key)
            .bind(delta_cents)
            .bind(&now_iso)
            .bind(&now_iso)
            .execute(&mut *conn)
            .await?;
            Ok(())
        }
    }
}

/// Inserts or updates the drawer state inside `unit`, preserving `created_at` and incrementing `version`.
pub async fn upsert_drawer_state<'a>(
    unit: impl Into<Unit<'a>>,
    mut state: DrawerStateDocument,
) -> AppResult<DrawerStateDocument> {
    state.key = state.user_key.clone();
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(mongo_db, mut session) => {
            let col = drawer_state_col(mongo_db);
            let filter = doc! { "user_key": &state.user_key };
            if let Some(prev) =
                crate::in_session!(col.find_one(filter.clone()), session.as_deref_mut())?
            {
                state.id = prev.id;
                state.created_at = prev.created_at;
                state.version = prev.version + 1;
                col.replace_one_in(filter, &state, session).await?;
            } else {
                state.version = 1;
                let res = col.insert_one_in(&state, session).await?;
                state.id = res.inserted_id.as_object_id();
            }
            Ok(state)
        }
        Backend::Sqlite(mut conn) => {
            let id_hex = match state.id {
                Some(oid) => oid.to_hex(),
                None => generate_id_hex(),
            };
            let created_at_iso = bson_to_iso(&state.created_at);
            let updated_at_iso = bson_to_iso(&state.updated_at);
            sqlx::query(
                r#"
                INSERT INTO drawer_state (
                    key, id, user_key, current_balance_cents, last_session_key,
                    last_closing_float_cents, version, created_at, updated_at, updated_by_device
                )
                VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?)
                ON CONFLICT(user_key) DO UPDATE SET
                    key = excluded.key,
                    current_balance_cents = excluded.current_balance_cents,
                    last_session_key = excluded.last_session_key,
                    last_closing_float_cents = excluded.last_closing_float_cents,
                    version = drawer_state.version + 1,
                    updated_at = excluded.updated_at,
                    updated_by_device = excluded.updated_by_device
                "#,
            )
            .bind(&state.key)
            .bind(&id_hex)
            .bind(&state.user_key)
            .bind(state.current_balance_cents)
            .bind(&state.last_session_key)
            .bind(state.last_closing_float_cents)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .bind(&state.updated_by_device)
            .execute(&mut *conn)
            .await?;

            let query = format!("SELECT {SQLITE_SELECT_COLS} FROM drawer_state WHERE user_key = ?");
            let row = sqlx::query(&query)
                .bind(&state.user_key)
                .fetch_one(&mut *conn)
                .await?;
            drawer_state_from_row(&row)
        }
    }
}
