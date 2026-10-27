// Dual-engine (Mongo / SQLite) access for the `recurring_expenses` collection.

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
    domain::expenses::PaidFrom,
    modules::recurring_expenses::model::RecurringExpenseDocument,
};

fn templates(db: &TenantDatabase) -> ScopedCollection<RecurringExpenseDocument> {
    db.collection("recurring_expenses")
}

fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<RecurringExpenseDocument> {
    let from: String = row.get("paid_from");
    Ok(RecurringExpenseDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        category_key: row.get("category_key"),
        payee_key: row.get("payee_key"),
        amount_cents: row.get("amount_cents"),
        due_day: row.get("due_day"),
        remind_days_before: row.get("remind_days_before"),
        paid_from: PaidFrom::parse(&from).ok_or_else(|| {
            AppError::internal(format!("Unknown account '{from}' on a regular bill"))
        })?,
        note: row.get("note"),
        active: row.get::<i64, _>("active") != 0,
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Every regular bill, stopped ones included; the service filters.
pub(crate) async fn list_all(db: &Db) -> AppResult<Vec<RecurringExpenseDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = templates(db).find(doc! {}).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM recurring_expenses")
                .fetch_all(pool)
                .await?;
            rows.iter().map(from_row).collect()
        }
    }
}

/// One regular bill by key.
pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<RecurringExpenseDocument>> {
    match db {
        Db::Mongo(db) => Ok(templates(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM recurring_expenses WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(from_row).transpose()
        }
    }
}

/// Inserts a new regular bill.
pub(crate) async fn insert(
    db: &Db,
    mut document: RecurringExpenseDocument,
) -> AppResult<RecurringExpenseDocument> {
    match db {
        Db::Mongo(db) => {
            let res = templates(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO recurring_expenses (
                    key, id, category_key, payee_key, amount_cents, due_day, remind_days_before,
                    paid_from, note, active, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.category_key)
            .bind(&document.payee_key)
            .bind(document.amount_cents)
            .bind(document.due_day)
            .bind(document.remind_days_before)
            .bind(document.paid_from.as_str())
            .bind(&document.note)
            .bind(i64::from(document.active))
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

/// Writes the editable fields of an existing regular bill back (the service bumped `version`/`updated_at`).
pub(crate) async fn save(db: &Db, document: &RecurringExpenseDocument) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            templates(db)
                .replace_one(doc! { "key": &document.key }, document)
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            sqlx::query(
                "UPDATE recurring_expenses SET amount_cents = ?, due_day = ?, remind_days_before = ?,
                    paid_from = ?, note = ?, active = ?, version = ?, updated_at = ? WHERE key = ?",
            )
            .bind(document.amount_cents)
            .bind(document.due_day)
            .bind(document.remind_days_before)
            .bind(document.paid_from.as_str())
            .bind(&document.note)
            .bind(i64::from(document.active))
            .bind(document.version)
            .bind(bson_to_iso(&document.updated_at))
            .bind(&document.key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}
