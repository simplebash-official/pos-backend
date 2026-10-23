// Dual-engine (Mongo / SQLite) access for `balance_openings` and `advance_recoveries`.

use futures_util::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
        tenant_db::{ScopedCollection, TenantDatabase},
    },
    core::error::{AppError, AppResult},
    domain::balances::BalanceSubject,
    modules::balances::model::{AdvanceRecoveryDocument, BalanceOpeningDocument},
};

fn openings(db: &TenantDatabase) -> ScopedCollection<BalanceOpeningDocument> {
    db.collection("balance_openings")
}

fn recoveries(db: &TenantDatabase) -> ScopedCollection<AdvanceRecoveryDocument> {
    db.collection("advance_recoveries")
}

fn opening_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<BalanceOpeningDocument> {
    let kind: String = row.get("subject_kind");
    Ok(BalanceOpeningDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        subject_kind: BalanceSubject::parse(&kind)
            .ok_or_else(|| AppError::internal(format!("Unknown balance subject '{kind}' in database")))?,
        subject_key: row.get("subject_key"),
        opening_balance_cents: row.get("opening_balance_cents"),
        as_of_date: row.get("as_of_date"),
        set_by_name_snapshot: row.get("set_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

fn recovery_from_row(row: &sqlx::sqlite::SqliteRow) -> AdvanceRecoveryDocument {
    AdvanceRecoveryDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        employee_key: row.get("employee_key"),
        amount_cents: row.get("amount_cents"),
        recovered_on: row.get("recovered_on"),
        note: row.get("note"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    }
}

/// Every opening of one kind (suppliers or staff).
pub(crate) async fn list_openings(db: &Db, kind: BalanceSubject) -> AppResult<Vec<BalanceOpeningDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = openings(db).find(doc! { "subject_kind": kind.as_str() }).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM balance_openings WHERE subject_kind = ?")
                .bind(kind.as_str())
                .fetch_all(pool)
                .await?;
            rows.iter().map(opening_from_row).collect()
        }
    }
}

/// One opening by key.
pub(crate) async fn find_opening(db: &Db, key: &str) -> AppResult<Option<BalanceOpeningDocument>> {
    match db {
        Db::Mongo(db) => Ok(openings(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM balance_openings WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(opening_from_row).transpose()
        }
    }
}

/// Inserts the first opening of a supplier or staff member.
pub(crate) async fn insert_opening(
    db: &Db,
    mut document: BalanceOpeningDocument,
) -> AppResult<BalanceOpeningDocument> {
    match db {
        Db::Mongo(db) => {
            let res = openings(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document.id.map(|o| o.to_hex()).unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO balance_openings (
                    key, id, subject_kind, subject_key, opening_balance_cents, as_of_date,
                    set_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(document.subject_kind.as_str())
            .bind(&document.subject_key)
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

/// Writes a changed opening back (the service bumped `version`/`updated_at`).
pub(crate) async fn save_opening(db: &Db, document: &BalanceOpeningDocument) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            openings(db).replace_one(doc! { "key": &document.key }, document).await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            sqlx::query(
                "UPDATE balance_openings SET opening_balance_cents = ?, as_of_date = ?,
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

/// Inserts a recovery.
pub(crate) async fn insert_recovery(
    db: &Db,
    mut document: AdvanceRecoveryDocument,
) -> AppResult<AdvanceRecoveryDocument> {
    match db {
        Db::Mongo(db) => {
            let res = recoveries(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document.id.map(|o| o.to_hex()).unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO advance_recoveries (
                    key, id, employee_key, amount_cents, recovered_on, note, recorded_by_user_id,
                    recorded_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.employee_key)
            .bind(document.amount_cents)
            .bind(&document.recovered_on)
            .bind(&document.note)
            .bind(&document.recorded_by_user_id)
            .bind(&document.recorded_by_name_snapshot)
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

/// Every recovery, oldest first; the service groups them per employee.
pub(crate) async fn list_recoveries(db: &Db) -> AppResult<Vec<AdvanceRecoveryDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = recoveries(db).find(doc! {}).sort(doc! { "created_at": 1 }).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM advance_recoveries ORDER BY created_at ASC")
                .fetch_all(pool)
                .await?;
            Ok(rows.iter().map(recovery_from_row).collect())
        }
    }
}
