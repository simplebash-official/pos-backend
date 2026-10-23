// Dual-engine (Mongo / SQLite) access for the `account_openings` collection.

use mongodb::bson::{doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
        tenant_db::{ScopedCollection, TenantDatabase},
    },
    core::error::{AppError, AppResult},
    domain::accounts::AccountKind,
    modules::accounts::model::AccountOpeningDocument,
};

fn openings(db: &TenantDatabase) -> ScopedCollection<AccountOpeningDocument> {
    db.collection("account_openings")
}

fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<AccountOpeningDocument> {
    let kind: String = row.get("kind");
    Ok(AccountOpeningDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        kind: AccountKind::parse(&kind)
            .ok_or_else(|| AppError::internal(format!("Unknown account '{kind}' in database")))?,
        opening_balance_cents: row.get("opening_balance_cents"),
        as_of_date: row.get("as_of_date"),
        set_by_name_snapshot: row.get("set_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// The opening balance row of one account, if it has been set.
pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<AccountOpeningDocument>> {
    match db {
        Db::Mongo(db) => Ok(openings(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM account_openings WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(from_row).transpose()
        }
    }
}

/// Inserts the first opening balance of an account.
pub(crate) async fn insert(db: &Db, mut document: AccountOpeningDocument) -> AppResult<AccountOpeningDocument> {
    match db {
        Db::Mongo(db) => {
            let res = openings(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document.id.map(|o| o.to_hex()).unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO account_openings (
                    key, id, kind, opening_balance_cents, as_of_date, set_by_name_snapshot,
                    version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(document.kind.as_str())
            .bind(document.opening_balance_cents)
            .bind(&document.as_of_date)
            .bind(&document.set_by_name_snapshot)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .execute(pool)
            .await?;
            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

/// Writes a changed opening balance back (the service bumped `version`/`updated_at`).
pub(crate) async fn save(db: &Db, document: &AccountOpeningDocument) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            openings(db).replace_one(doc! { "key": &document.key }, document).await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            sqlx::query(
                "UPDATE account_openings SET opening_balance_cents = ?, as_of_date = ?,
                    set_by_name_snapshot = ?, version = ?, updated_at = ? WHERE key = ?",
            )
            .bind(document.opening_balance_cents)
            .bind(&document.as_of_date)
            .bind(&document.set_by_name_snapshot)
            .bind(document.version)
            .bind(bson_to_iso(&document.updated_at))
            .bind(&document.key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}
