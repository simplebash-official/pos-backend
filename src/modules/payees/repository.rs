// Dual-engine (Mongo / SQLite) access for the `payees` collection.

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
    domain::cash_categories::PayeeType,
    modules::payees::model::PayeeDocument,
};

fn payees(db: &TenantDatabase) -> ScopedCollection<PayeeDocument> {
    db.collection("payees")
}

fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<PayeeDocument> {
    let type_str: String = row.get("payee_type");
    Ok(PayeeDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        payee_type: PayeeType::parse(&type_str)
            .ok_or_else(|| AppError::internal(format!("Unknown payee type '{type_str}' in database")))?,
        name: row.get("name"),
        normalized_name: row.get("normalized_name"),
        phone: row.get("phone"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Every stored payee.
pub(crate) async fn list_all(db: &Db) -> AppResult<Vec<PayeeDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = payees(db).find(doc! {}).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM payees").fetch_all(pool).await?;
            rows.iter().map(from_row).collect()
        }
    }
}

/// One stored payee by key.
pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<PayeeDocument>> {
    match db {
        Db::Mongo(db) => Ok(payees(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM payees WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(from_row).transpose()
        }
    }
}

/// Inserts a new payee.
pub(crate) async fn insert(db: &Db, mut document: PayeeDocument) -> AppResult<PayeeDocument> {
    match db {
        Db::Mongo(db) => {
            let res = payees(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document.id.map(|o| o.to_hex()).unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO payees (key, id, payee_type, name, normalized_name, phone, version, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(document.payee_type.as_str())
            .bind(&document.name)
            .bind(&document.normalized_name)
            .bind(&document.phone)
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
