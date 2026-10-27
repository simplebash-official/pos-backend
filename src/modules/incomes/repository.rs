// Dual-engine (Mongo / SQLite) access for the `incomes` collection.
// Append-only: rows are inserted and read, never edited or deleted.

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
    domain::incomes::ReceivedInto,
    modules::incomes::model::IncomeDocument,
};

fn incomes(db: &TenantDatabase) -> ScopedCollection<IncomeDocument> {
    db.collection("incomes")
}

fn income_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<IncomeDocument> {
    let into: String = row.get("received_into");
    Ok(IncomeDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        income_number: row.get("income_number"),
        income_date: row.get("income_date"),
        category_key: row.get("category_key"),
        payee_key: row.get("payee_key"),
        payee_name: row.get("payee_name"),
        amount_cents: row.get("amount_cents"),
        received_into: ReceivedInto::parse(&into)
            .ok_or_else(|| AppError::internal(format!("Unknown account '{into}' in database")))?,
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

/// Inserts a new income.
pub(crate) async fn insert_income(
    db: &Db,
    mut document: IncomeDocument,
) -> AppResult<IncomeDocument> {
    match db {
        Db::Mongo(db) => {
            let res = incomes(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO incomes (
                    key, id, income_number, income_date, category_key, payee_key, payee_name,
                    amount_cents, received_into, reference_no, note, recorded_by_user_id,
                    recorded_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.income_number)
            .bind(&document.income_date)
            .bind(&document.category_key)
            .bind(&document.payee_key)
            .bind(&document.payee_name)
            .bind(document.amount_cents)
            .bind(document.received_into.as_str())
            .bind(&document.reference_no)
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

/// One income by key.
pub(crate) async fn find_income_by_key(db: &Db, key: &str) -> AppResult<Option<IncomeDocument>> {
    match db {
        Db::Mongo(db) => Ok(incomes(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM incomes WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(income_from_row).transpose()
        }
    }
}

/// Every income dated between two days (`YYYY-MM-DD`, inclusive; `None` is open-ended).
pub(crate) async fn list_incomes(
    db: &Db,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<IncomeDocument>> {
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
                doc! { "income_date": range }
            };
            let mut cursor = incomes(db).find(filter).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let mut sql = String::from("SELECT * FROM incomes WHERE 1 = 1");
            if from.is_some() {
                sql.push_str(" AND income_date >= ?");
            }
            if to.is_some() {
                sql.push_str(" AND income_date <= ?");
            }
            let mut query = sqlx::query(&sql);
            if let Some(from) = from {
                query = query.bind(from);
            }
            if let Some(to) = to {
                query = query.bind(to);
            }
            query
                .fetch_all(pool)
                .await?
                .iter()
                .map(income_from_row)
                .collect()
        }
    }
}
