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
    core::error::{AppError, AppResult},
    domain::sessions::{CashDirection, CashMovementListQuery, CashMovementType},
    modules::sessions::model::CashMovementDocument,
};

fn cash_movements_col(db: &TenantDatabase) -> ScopedCollection<CashMovementDocument> {
    db.collection("cash_movements")
}

pub(crate) fn movement_from_sqlite_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<CashMovementDocument> {
    let id_str: String = row.get("id");
    let movement_type_str: String = row.get("movement_type");
    let movement_type = CashMovementType::parse(&movement_type_str).ok_or_else(|| {
        AppError::internal(format!("Unknown cash movement type '{movement_type_str}' in database"))
    })?;
    let direction_str: String = row.get("direction");
    let direction = CashDirection::parse(&direction_str).ok_or_else(|| {
        AppError::internal(format!("Unknown cash movement direction '{direction_str}' in database"))
    })?;
    let recorded_late_int: i64 = row.get("recorded_late");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(CashMovementDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: row.get("key"),
        movement_number: row.get("movement_number"),
        movement_type,
        direction,
        amount_cents: row.get("amount_cents"),
        session_key: row.get("session_key"),
        terminal_id: row.get("terminal_id"),
        category_key: row.get("category_key"),
        custom_label: row.get("custom_label"),
        payee_key: row.get("payee_key"),
        payee_name: row.get("payee_name"),
        period_start: row.get("period_start"),
        period_end: row.get("period_end"),
        reference_no: row.get("reference_no"),
        note: row.get("note"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        approved_by_user_id: row.get("approved_by_user_id"),
        approved_by_name_snapshot: row.get("approved_by_name_snapshot"),
        approval_reason: row.get("approval_reason"),
        reverses_key: row.get("reverses_key"),
        recorded_late: recorded_late_int != 0,
        original_variance_session_key: row.get("original_variance_session_key"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}

/// Inserts one movement row (append-only: movements are never edited or deleted).
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
                    key, id, movement_number, movement_type, direction, amount_cents, session_key,
                    terminal_id, category_key, custom_label, payee_key, payee_name,
                    period_start, period_end, reference_no, note,
                    recorded_by_user_id, recorded_by_name_snapshot,
                    approved_by_user_id, approved_by_name_snapshot,
                    approval_reason, reverses_key,
                    recorded_late, original_variance_session_key,
                    version, created_at, updated_at
                ) VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                )
                "#
            )
            .bind(&doc.key)
            .bind(&id)
            .bind(&doc.movement_number)
            .bind(doc.movement_type.as_str())
            .bind(doc.direction.as_str())
            .bind(doc.amount_cents)
            .bind(&doc.session_key)
            .bind(&doc.terminal_id)
            .bind(&doc.category_key)
            .bind(&doc.custom_label)
            .bind(&doc.payee_key)
            .bind(&doc.payee_name)
            .bind(&doc.period_start)
            .bind(&doc.period_end)
            .bind(&doc.reference_no)
            .bind(&doc.note)
            .bind(&doc.recorded_by_user_id)
            .bind(&doc.recorded_by_name_snapshot)
            .bind(&doc.approved_by_user_id)
            .bind(&doc.approved_by_name_snapshot)
            .bind(&doc.approval_reason)
            .bind(&doc.reverses_key)
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
            if let Some(category_keys) = &query.category_keys {
                filter.insert("category_key", doc! { "$in": category_keys });
            }
            if let Some(payee_key) = &query.payee_key {
                filter.insert("payee_key", payee_key);
            }
            if let Some(recorded_late) = query.recorded_late {
                filter.insert("recorded_late", recorded_late);
            }
            if let Some(from) = &query.from {
                filter.insert("created_at", doc! { "$gte": from });
            }
            if let Some(to) = &query.to {
                filter.entry("created_at".to_string()).or_insert_with(|| doc! {}.into());
                if let Ok(d) = filter.get_document_mut("created_at") {
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
            if let Some(category_keys) = &query.category_keys {
                let placeholders = category_keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                conditions.push(format!("category_key IN ({placeholders})"));
                bindings.extend(category_keys.iter().cloned());
            }
            if let Some(payee_key) = &query.payee_key {
                conditions.push("payee_key = ?".to_string());
                bindings.push(payee_key.clone());
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

/// Movements per category key, for ranking the picker by this shop's real usage.
pub async fn count_movements_by_category(db: &Db) -> AppResult<Vec<(String, i64)>> {
    count_by(db, "category_key").await
}

/// Movements per saved payee key, so the payee combobox can put frequent payees first.
pub async fn count_movements_by_payee(db: &Db) -> AppResult<Vec<(String, i64)>> {
    count_by(db, "payee_key").await
}

async fn count_by(db: &Db, field: &'static str) -> AppResult<Vec<(String, i64)>> {
    match db {
        Db::Mongo(mongo_db) => {
            let pipeline = vec![
                doc! { "$match": { field: { "$type": "string" } } },
                doc! { "$group": { "_id": format!("${field}"), "n": { "$sum": 1 } } },
            ];
            let mut cursor = cash_movements_col(mongo_db).aggregate(pipeline).await?;
            let mut out = Vec::new();
            while let Some(row) = cursor.try_next().await? {
                let key = row.get_str("_id").map_err(|e| AppError::internal(e.to_string()))?;
                let n = match row.get("n") {
                    Some(mongodb::bson::Bson::Int32(n)) => i64::from(*n),
                    Some(mongodb::bson::Bson::Int64(n)) => *n,
                    _ => 0,
                };
                out.push((key.to_string(), n));
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let sql = format!(
                "SELECT {field} AS k, COUNT(*) AS n FROM cash_movements \
                 WHERE {field} IS NOT NULL AND deleted_at IS NULL GROUP BY {field}"
            );
            let rows = sqlx::query(&sql).fetch_all(pool).await?;
            Ok(rows.iter().map(|r| (r.get("k"), r.get("n"))).collect())
        }
    }
}

/// Movements of the given types recorded on or between two days (`YYYY-MM-DD`, UTC, inclusive;
/// `None` is open-ended), oldest first. For the expense ledger, which reads drawer payments
/// straight from here.
pub async fn list_movements_of_types(
    db: &Db,
    types: &[CashMovementType],
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<CashMovementDocument>> {
    let type_names: Vec<&str> = types.iter().map(CashMovementType::as_str).collect();
    match db {
        Db::Mongo(mongo_db) => {
            let mut filter = doc! { "movement_type": { "$in": &type_names } };
            let mut range = doc! {};
            let day_start = |day: &str| -> AppResult<mongodb::bson::DateTime> {
                let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
                    .map_err(|_| AppError::validation("Please pick a valid date."))?;
                Ok(mongodb::bson::DateTime::from_chrono(
                    date.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc(),
                ))
            };
            if let Some(from) = from {
                range.insert("$gte", day_start(from)?);
            }
            if let Some(to) = to {
                // Inclusive end day: everything before the next midnight.
                let next = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")
                    .map_err(|_| AppError::validation("Please pick a valid date."))?
                    .succ_opt()
                    .ok_or_else(|| AppError::validation("Please pick a valid date."))?;
                range.insert("$lt", day_start(&next.format("%Y-%m-%d").to_string())?);
            }
            if !range.is_empty() {
                filter.insert("created_at", range);
            }
            let mut cursor = cash_movements_col(mongo_db)
                .find(filter)
                .sort(doc! { "created_at": 1 })
                .await?;
            let mut out = Vec::new();
            while let Some(item) = cursor.try_next().await? {
                out.push(item);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let placeholders = type_names.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let mut sql = format!(
                "SELECT * FROM cash_movements WHERE deleted_at IS NULL AND movement_type IN ({placeholders})"
            );
            if from.is_some() {
                sql.push_str(" AND substr(created_at, 1, 10) >= ?");
            }
            if to.is_some() {
                sql.push_str(" AND substr(created_at, 1, 10) <= ?");
            }
            sql.push_str(" ORDER BY created_at ASC");
            let mut query = sqlx::query(&sql);
            for name in &type_names {
                query = query.bind(*name);
            }
            if let Some(from) = from {
                query = query.bind(from);
            }
            if let Some(to) = to {
                query = query.bind(to);
            }
            let rows = query.fetch_all(pool).await?;
            rows.iter().map(movement_from_sqlite_row).collect()
        }
    }
}

/// For each of the given movement keys, the key of the reversal that cancels it, if one exists.
pub async fn find_reversals_of(db: &Db, keys: &[String]) -> AppResult<Vec<(String, String)>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(mongo_db) => {
            let mut cursor = cash_movements_col(mongo_db)
                .find(doc! { "reverses_key": { "$in": keys } })
                .await?;
            let mut out = Vec::new();
            while let Some(item) = cursor.try_next().await? {
                if let Some(original) = item.reverses_key {
                    out.push((original, item.key));
                }
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let placeholders = keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT reverses_key, key FROM cash_movements WHERE reverses_key IN ({placeholders}) AND deleted_at IS NULL"
            );
            let mut query = sqlx::query(&sql);
            for key in keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            Ok(rows.iter().map(|r| (r.get("reverses_key"), r.get("key"))).collect())
        }
    }
}
