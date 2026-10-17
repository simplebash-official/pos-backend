// Cash movement database operations for SQLite and MongoDB.

use futures_util::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
        tenant_db::{ScopedCollection, TenantDatabase},
    },
    core::error::AppResult,
    domain::sessions::{CashMovementListQuery, CashMovementType},
    modules::sessions::model::CashMovementDocument,
};

fn cash_movements_col(db: &TenantDatabase) -> ScopedCollection<CashMovementDocument> {
    db.collection("cash_movements")
}

pub(crate) fn movement_from_sqlite_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<CashMovementDocument> {
    let id_str: String = row.get("id");
    let movement_type_str: String = row.get("movement_type");
    let movement_type = CashMovementType::from_str(&movement_type_str)
        .unwrap_or(CashMovementType::PayIn);
    let recorded_late_int: i64 = row.get("recorded_late");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(CashMovementDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: row.get("key"),
        movement_number: row.get("movement_number"),
        movement_type,
        amount_cents: row.get("amount_cents"),
        session_key: row.get("session_key"),
        terminal_id: row.get("terminal_id"),
        reason: row.get("reason"),
        recipient_or_source: row.get("recipient_or_source"),
        supplier_key: row.get("supplier_key"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        approved_by_user_id: row.get("approved_by_user_id"),
        approved_by_name_snapshot: row.get("approved_by_name_snapshot"),
        recorded_late: recorded_late_int != 0,
        original_variance_session_key: row.get("original_variance_session_key"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}

pub async fn insert_cash_movement(
    db: &Db,
    mut doc: CashMovementDocument,
) -> AppResult<CashMovementDocument> {
    match db {
        Db::Mongo(mongo_db) => {
            let res = cash_movements_col(mongo_db).insert_one(&doc).await?;
            doc.id = res.inserted_id.as_object_id();
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let id = doc
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let created_at_iso = bson_to_iso(&doc.created_at);
            let updated_at_iso = bson_to_iso(&doc.updated_at);

            sqlx::query(
                r#"
                INSERT INTO cash_movements (
                    key, id, movement_number, movement_type, amount_cents, session_key,
                    terminal_id, reason, recipient_or_source, supplier_key,
                    recorded_by_user_id, recorded_by_name_snapshot,
                    approved_by_user_id, approved_by_name_snapshot,
                    recorded_late, original_variance_session_key,
                    version, created_at, updated_at
                ) VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                )
                "#
            )
            .bind(&doc.key)
            .bind(&id)
            .bind(&doc.movement_number)
            .bind(doc.movement_type.as_str())
            .bind(doc.amount_cents)
            .bind(&doc.session_key)
            .bind(&doc.terminal_id)
            .bind(&doc.reason)
            .bind(&doc.recipient_or_source)
            .bind(&doc.supplier_key)
            .bind(&doc.recorded_by_user_id)
            .bind(&doc.recorded_by_name_snapshot)
            .bind(&doc.approved_by_user_id)
            .bind(&doc.approved_by_name_snapshot)
            .bind(if doc.recorded_late { 1 } else { 0 })
            .bind(&doc.original_variance_session_key)
            .bind(doc.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .execute(pool)
            .await?;

            doc.id = ObjectId::parse_str(&id).ok();
            Ok(doc)
        }
    }
}

pub async fn find_cash_movement_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<CashMovementDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let doc = cash_movements_col(mongo_db)
                .find_one(doc! { "key": key })
                .await?;
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM cash_movements WHERE key = ? AND deleted_at IS NULL"
            )
            .bind(key)
            .fetch_optional(pool)
            .await?;

            row.map(|r| movement_from_sqlite_row(&r)).transpose()
        }
    }
}

pub async fn list_cash_movements_for_session(
    db: &Db,
    session_key: &str,
) -> AppResult<Vec<CashMovementDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let mut cursor = cash_movements_col(mongo_db)
                .find(doc! { "session_key": session_key })
                .sort(doc! { "created_at": 1 })
                .await?;
            let mut results = Vec::new();
            while let Some(item) = cursor.try_next().await? {
                results.push(item);
            }
            Ok(results)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM cash_movements WHERE session_key = ? AND deleted_at IS NULL ORDER BY created_at ASC"
            )
            .bind(session_key)
            .fetch_all(pool)
            .await?;

            let mut results = Vec::with_capacity(rows.len());
            for r in rows {
                results.push(movement_from_sqlite_row(&r)?);
            }
            Ok(results)
        }
    }
}

pub async fn list_cash_movements(
    db: &Db,
    query: &CashMovementListQuery,
) -> AppResult<(Vec<CashMovementDocument>, u64)> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(50).min(200);
    let skip = (page - 1) * limit;

    match db {
        Db::Mongo(mongo_db) => {
            let mut filter = doc! {};
            if let Some(session_key) = &query.session_key {
                filter.insert("session_key", session_key);
            }
            if let Some(terminal_id) = &query.terminal_id {
                filter.insert("terminal_id", terminal_id);
            }
            if let Some(movement_type) = &query.movement_type {
                filter.insert("movement_type", movement_type);
            }
            if let Some(recorded_late) = query.recorded_late {
                filter.insert("recorded_late", recorded_late);
            }
            if let Some(from) = &query.from {
                filter.insert("created_at", doc! { "$gte": from });
            }
            if let Some(to) = &query.to {
                filter.entry("created_at".to_string()).or_insert_with(|| doc! {}.into());
                if let Some(d) = filter.get_document_mut("created_at").ok() {
                    d.insert("$lte", to);
                }
            }

            let total = cash_movements_col(mongo_db).count_documents(filter.clone()).await?;
            let mut cursor = cash_movements_col(mongo_db)
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .skip(skip)
                .limit(limit as i64)
                .await?;

            let mut results = Vec::new();
            while let Some(item) = cursor.try_next().await? {
                results.push(item);
            }
            Ok((results, total))
        }
        Db::Sqlite(pool) => {
            let mut conditions = vec!["deleted_at IS NULL".to_string()];
            let mut bindings: Vec<String> = Vec::new();

            if let Some(session_key) = &query.session_key {
                conditions.push("session_key = ?".to_string());
                bindings.push(session_key.clone());
            }
            if let Some(terminal_id) = &query.terminal_id {
                conditions.push("terminal_id = ?".to_string());
                bindings.push(terminal_id.clone());
            }
            if let Some(movement_type) = &query.movement_type {
                conditions.push("movement_type = ?".to_string());
                bindings.push(movement_type.clone());
            }
            if let Some(recorded_late) = query.recorded_late {
                conditions.push("recorded_late = ?".to_string());
                bindings.push(if recorded_late { "1".to_string() } else { "0".to_string() });
            }
            if let Some(from) = &query.from {
                conditions.push("created_at >= ?".to_string());
                bindings.push(from.clone());
            }
            if let Some(to) = &query.to {
                conditions.push("created_at <= ?".to_string());
                bindings.push(to.clone());
            }

            let where_clause = conditions.join(" AND ");
            let count_sql = format!("SELECT COUNT(*) as total FROM cash_movements WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let count_row = count_query.fetch_one(pool).await?;
            let total: i64 = count_row.get("total");

            let select_sql = format!(
                "SELECT * FROM cash_movements WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            select_query = select_query.bind(limit as i64).bind(skip as i64);

            let rows = select_query.fetch_all(pool).await?;
            let mut results = Vec::with_capacity(rows.len());
            for r in rows {
                results.push(movement_from_sqlite_row(&r)?);
            }
            Ok((results, total as u64))
        }
    }
}
