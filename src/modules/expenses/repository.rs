// Dual-engine (Mongo / SQLite) access for the `expenses` and `expense_payments` collections.
// Both are append-only: rows are inserted and read, never edited or deleted.

use futures_util::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
        tenant_db::{ScopedCollection, TenantDatabase},
        uow::{Backend, Unit},
    },
    core::error::{AppError, AppResult},
    domain::expenses::PaidFrom,
    modules::expenses::model::{ExpenseDocument, ExpensePaymentDocument},
};

fn expenses(db: &TenantDatabase) -> ScopedCollection<ExpenseDocument> {
    db.collection("expenses")
}

fn payments(db: &TenantDatabase) -> ScopedCollection<ExpensePaymentDocument> {
    db.collection("expense_payments")
}

fn expense_from_row(row: &sqlx::sqlite::SqliteRow) -> ExpenseDocument {
    ExpenseDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        expense_number: row.get("expense_number"),
        expense_date: row.get("expense_date"),
        category_key: row.get("category_key"),
        payee_key: row.get("payee_key"),
        payee_name: row.get("payee_name"),
        amount_cents: row.get("amount_cents"),
        due_date: row.get("due_date"),
        reference_no: row.get("reference_no"),
        note: row.get("note"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    }
}

fn payment_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<ExpensePaymentDocument> {
    let from: String = row.get("paid_from");
    Ok(ExpensePaymentDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        expense_key: row.get("expense_key"),
        amount_cents: row.get("amount_cents"),
        paid_from: PaidFrom::parse(&from).ok_or_else(|| {
            AppError::internal(format!("Unknown payment account '{from}' in database"))
        })?,
        paid_on: row.get("paid_on"),
        reference_no: row.get("reference_no"),
        note: row.get("note"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Inserts a new expense.
pub(crate) async fn insert_expense(
    unit: impl Into<Unit<'_>>,
    mut document: ExpenseDocument,
) -> AppResult<ExpenseDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let res = expenses(db).insert_one_in(&document, session).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO expenses (
                    key, id, expense_number, expense_date, category_key, payee_key, payee_name,
                    amount_cents, due_date, reference_no, note, recorded_by_user_id,
                    recorded_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.expense_number)
            .bind(&document.expense_date)
            .bind(&document.category_key)
            .bind(&document.payee_key)
            .bind(&document.payee_name)
            .bind(document.amount_cents)
            .bind(&document.due_date)
            .bind(&document.reference_no)
            .bind(&document.note)
            .bind(&document.recorded_by_user_id)
            .bind(&document.recorded_by_name_snapshot)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .execute(&mut *conn)
            .await?;
            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

/// Inserts a payment against an expense.
pub(crate) async fn insert_payment(
    unit: impl Into<Unit<'_>>,
    mut document: ExpensePaymentDocument,
) -> AppResult<ExpensePaymentDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let res = payments(db).insert_one_in(&document, session).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO expense_payments (
                    key, id, expense_key, amount_cents, paid_from, paid_on, reference_no, note,
                    recorded_by_user_id, recorded_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.expense_key)
            .bind(document.amount_cents)
            .bind(document.paid_from.as_str())
            .bind(&document.paid_on)
            .bind(&document.reference_no)
            .bind(&document.note)
            .bind(&document.recorded_by_user_id)
            .bind(&document.recorded_by_name_snapshot)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .execute(&mut *conn)
            .await?;
            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

/// One expense by key.
pub(crate) async fn find_expense_by_key(db: &Db, key: &str) -> AppResult<Option<ExpenseDocument>> {
    match db {
        Db::Mongo(db) => Ok(expenses(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM expenses WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            Ok(row.as_ref().map(expense_from_row))
        }
    }
}

/// Every expense dated between two days (`YYYY-MM-DD`, inclusive; `None` is open-ended).
pub(crate) async fn list_expenses(
    db: &Db,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<ExpenseDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut range = doc! {};
            if let Some(from) = from {
                range.insert("$gte", from);
            }
            if let Some(to) = to {
                range.insert("$lte", to);
            }
            let filter = if range.is_empty() {
                doc! {}
            } else {
                doc! { "expense_date": range }
            };
            let mut cursor = expenses(db).find(filter).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let mut sql = String::from("SELECT * FROM expenses WHERE 1 = 1");
            if from.is_some() {
                sql.push_str(" AND expense_date >= ?");
            }
            if to.is_some() {
                sql.push_str(" AND expense_date <= ?");
            }
            let mut query = sqlx::query(&sql);
            if let Some(from) = from {
                query = query.bind(from);
            }
            if let Some(to) = to {
                query = query.bind(to);
            }
            Ok(query
                .fetch_all(pool)
                .await?
                .iter()
                .map(expense_from_row)
                .collect())
        }
    }
}

/// Every payment, oldest first. The ledger is shop-sized, so the service groups them in memory.
pub(crate) async fn list_all_payments(db: &Db) -> AppResult<Vec<ExpensePaymentDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = payments(db)
                .find(doc! {})
                .sort(doc! { "created_at": 1 })
                .await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM expense_payments ORDER BY created_at ASC")
                .fetch_all(pool)
                .await?;
            rows.iter().map(payment_from_row).collect()
        }
    }
}

/// The payments of one expense, oldest first.
pub(crate) async fn list_payments_for(
    db: &Db,
    expense_key: &str,
) -> AppResult<Vec<ExpensePaymentDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = payments(db)
                .find(doc! { "expense_key": expense_key })
                .sort(doc! { "created_at": 1 })
                .await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM expense_payments WHERE expense_key = ? ORDER BY created_at ASC",
            )
            .bind(expense_key)
            .fetch_all(pool)
            .await?;
            rows.iter().map(payment_from_row).collect()
        }
    }
}
