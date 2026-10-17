// Register session database operations for SQLite and MongoDB.

use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
        tenant_db::{ScopedCollection, TenantDatabase},
    },
    core::error::AppResult,
    domain::sessions::{CashMovementType, SessionListQuery, SessionStatus},
    modules::sessions::model::RegisterSessionDocument,
};

fn sessions_col(db: &TenantDatabase) -> ScopedCollection<RegisterSessionDocument> {
    db.collection("register_sessions")
}

pub(crate) fn session_from_sqlite_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<RegisterSessionDocument> {
    let id_str: String = row.get("id");
    let status_str: String = row.get("status");
    let status = SessionStatus::parse(&status_str).unwrap_or(SessionStatus::Open);

    let opening_denominations_str: Option<String> = row.get("opening_denominations");
    let opening_denominations = opening_denominations_str
        .and_then(|s| serde_json::from_str(&s).ok());

    let closing_denominations_str: Option<String> = row.get("closing_denominations");
    let closing_denominations = closing_denominations_str
        .and_then(|s| serde_json::from_str(&s).ok());

    let opened_at_str: String = row.get("opened_at");
    let closed_at_str: Option<String> = row.get("closed_at");
    let reopened_at_str: Option<String> = row.get("reopened_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let has_late_movements_int: i64 = row.get("has_late_movements");

    Ok(RegisterSessionDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: row.get("key"),
        session_number: row.get("session_number"),
        terminal_id: row.get("terminal_id"),
        cashier_id: row.get("cashier_id"),
        cashier_name_snapshot: row.get("cashier_name_snapshot"),
        status,
        opened_at: to_bson_datetime(&opened_at_str),
        opening_float_cents: row.get("opening_float_cents"),
        opening_denominations,
        opening_note: row.get("opening_note"),
        expected_opening_float_cents: row.get("expected_opening_float_cents"),
        opening_discrepancy_cents: row.get("opening_discrepancy_cents"),
        opening_discrepancy_reason: row.get("opening_discrepancy_reason"),
        opening_approved_by_id: row.get("opening_approved_by_id"),
        opening_approved_by_name: row.get("opening_approved_by_name"),
        closed_at: closed_at_str.as_deref().map(to_bson_datetime),
        closing_cashier_id: row.get("closing_cashier_id"),
        closing_cashier_name_snapshot: row.get("closing_cashier_name_snapshot"),
        expected_cash_cents: row.get("expected_cash_cents"),
        counted_cash_cents: row.get("counted_cash_cents"),
        cash_variance_cents: row.get("cash_variance_cents"),
        closing_denominations,
        closing_note: row.get("closing_note"),
        closing_float_cents: row.get("closing_float_cents"),
        cash_drop_to_safe_cents: row.get("cash_drop_to_safe_cents"),
        variance_reason: row.get("variance_reason"),
        variance_approved_by_id: row.get("variance_approved_by_id"),
        variance_approved_by_name: row.get("variance_approved_by_name"),
        reopened_at: reopened_at_str.as_deref().map(to_bson_datetime),
        reopened_by_id: row.get("reopened_by_id"),
        reopened_by_name: row.get("reopened_by_name"),
        reopened_reason: row.get("reopened_reason"),
        total_cash_sales_cents: row.get("total_cash_sales_cents"),
        total_card_sales_cents: row.get("total_card_sales_cents"),
        total_online_sales_cents: row.get("total_online_sales_cents"),
        total_credit_sales_cents: row.get("total_credit_sales_cents"),
        total_cash_refunds_cents: row.get("total_cash_refunds_cents"),
        total_pay_in_cents: row.get("total_pay_in_cents"),
        total_pay_out_cents: row.get("total_pay_out_cents"),
        total_owner_withdrawals_cents: row.get("total_owner_withdrawals_cents"),
        total_cash_drops_cents: row.get("total_cash_drops_cents"),
        has_late_movements: has_late_movements_int != 0,
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}

pub async fn insert_session(
    db: &Db,
    mut doc: RegisterSessionDocument,
) -> AppResult<RegisterSessionDocument> {
    match db {
        Db::Mongo(mongo_db) => {
            let res = sessions_col(mongo_db).insert_one(&doc).await?;
            doc.id = res.inserted_id.as_object_id();
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let id = doc
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let opened_at_iso = bson_to_iso(&doc.opened_at);
            let closed_at_iso = doc.closed_at.as_ref().map(bson_to_iso);
            let reopened_at_iso = doc.reopened_at.as_ref().map(bson_to_iso);
            let created_at_iso = bson_to_iso(&doc.created_at);
            let updated_at_iso = bson_to_iso(&doc.updated_at);
            let opening_denom_json = doc
                .opening_denominations
                .as_ref()
                .map(|v| serde_json::to_string(v).unwrap_or_default());
            let closing_denom_json = doc
                .closing_denominations
                .as_ref()
                .map(|v| serde_json::to_string(v).unwrap_or_default());

            sqlx::query(
                r#"
                INSERT INTO register_sessions (
                    key, id, session_number, terminal_id, cashier_id, cashier_name_snapshot,
                    status, opened_at, opening_float_cents, opening_denominations, opening_note,
                    expected_opening_float_cents, opening_discrepancy_cents, opening_discrepancy_reason,
                    opening_approved_by_id, opening_approved_by_name, closed_at,
                    closing_cashier_id, closing_cashier_name_snapshot,
                    expected_cash_cents, counted_cash_cents, cash_variance_cents,
                    closing_denominations, closing_note, closing_float_cents,
                    cash_drop_to_safe_cents, variance_reason, variance_approved_by_id,
                    variance_approved_by_name, reopened_at, reopened_by_id, reopened_by_name,
                    reopened_reason, total_cash_sales_cents, total_card_sales_cents,
                    total_online_sales_cents, total_credit_sales_cents, total_cash_refunds_cents,
                    total_pay_in_cents, total_pay_out_cents, total_owner_withdrawals_cents,
                    total_cash_drops_cents, has_late_movements, version, created_at, updated_at
                ) VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                    ?, ?, ?, ?, ?, ?
                )
                "#
            )
            .bind(&doc.key)
            .bind(&id)
            .bind(&doc.session_number)
            .bind(&doc.terminal_id)
            .bind(&doc.cashier_id)
            .bind(&doc.cashier_name_snapshot)
            .bind(doc.status.as_str())
            .bind(&opened_at_iso)
            .bind(doc.opening_float_cents)
            .bind(&opening_denom_json)
            .bind(&doc.opening_note)
            .bind(doc.expected_opening_float_cents)
            .bind(doc.opening_discrepancy_cents)
            .bind(&doc.opening_discrepancy_reason)
            .bind(&doc.opening_approved_by_id)
            .bind(&doc.opening_approved_by_name)
            .bind(&closed_at_iso)
            .bind(&doc.closing_cashier_id)
            .bind(&doc.closing_cashier_name_snapshot)
            .bind(doc.expected_cash_cents)
            .bind(doc.counted_cash_cents)
            .bind(doc.cash_variance_cents)
            .bind(&closing_denom_json)
            .bind(&doc.closing_note)
            .bind(doc.closing_float_cents)
            .bind(doc.cash_drop_to_safe_cents)
            .bind(&doc.variance_reason)
            .bind(&doc.variance_approved_by_id)
            .bind(&doc.variance_approved_by_name)
            .bind(&reopened_at_iso)
            .bind(&doc.reopened_by_id)
            .bind(&doc.reopened_by_name)
            .bind(&doc.reopened_reason)
            .bind(doc.total_cash_sales_cents)
            .bind(doc.total_card_sales_cents)
            .bind(doc.total_online_sales_cents)
            .bind(doc.total_credit_sales_cents)
            .bind(doc.total_cash_refunds_cents)
            .bind(doc.total_pay_in_cents)
            .bind(doc.total_pay_out_cents)
            .bind(doc.total_owner_withdrawals_cents)
            .bind(doc.total_cash_drops_cents)
            .bind(if doc.has_late_movements { 1 } else { 0 })
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

pub async fn find_session_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<RegisterSessionDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let doc = sessions_col(mongo_db)
                .find_one(doc! { "key": key })
                .await?;
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM register_sessions WHERE key = ? AND deleted_at IS NULL"
            )
            .bind(key)
            .fetch_optional(pool)
            .await?;

            row.map(|r| session_from_sqlite_row(&r)).transpose()
        }
    }
}

pub async fn find_session_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<RegisterSessionDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Db::Mongo(mongo_db) = db
            && let Some(doc) = sessions_col(mongo_db).find_one(doc! { "_id": object_id }).await? {
                return Ok(Some(doc));
            }
    match db {
        Db::Mongo(mongo_db) => {
            let doc = sessions_col(mongo_db)
                .find_one(doc! {
                    "$or": [
                        { "key": id_or_key },
                        { "session_number": id_or_key },
                    ]
                })
                .await?;
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM register_sessions WHERE (key = ? OR session_number = ? OR id = ?) AND deleted_at IS NULL"
            )
            .bind(id_or_key)
            .bind(id_or_key)
            .bind(id_or_key)
            .fetch_optional(pool)
            .await?;

            row.map(|r| session_from_sqlite_row(&r)).transpose()
        }
    }
}

pub async fn find_active_session_for_terminal(
    db: &Db,
    terminal_id: &str,
) -> AppResult<Option<RegisterSessionDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let doc = sessions_col(mongo_db)
                .find_one(doc! {
                    "terminal_id": terminal_id,
                    "status": { "$in": ["open", "closing", "reopened"] }
                })
                .await?;
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM register_sessions WHERE terminal_id = ? AND status IN ('open', 'closing', 'reopened') AND deleted_at IS NULL"
            )
            .bind(terminal_id)
            .fetch_optional(pool)
            .await?;

            row.map(|r| session_from_sqlite_row(&r)).transpose()
        }
    }
}

pub async fn find_active_session_for_cashier(
    db: &Db,
    cashier_id: &str,
) -> AppResult<Option<RegisterSessionDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let doc = sessions_col(mongo_db)
                .find_one(doc! {
                    "cashier_id": cashier_id,
                    "status": { "$in": ["open", "closing", "reopened"] }
                })
                .await?;
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM register_sessions WHERE cashier_id = ? AND status IN ('open', 'closing', 'reopened') AND deleted_at IS NULL"
            )
            .bind(cashier_id)
            .fetch_optional(pool)
            .await?;

            row.map(|r| session_from_sqlite_row(&r)).transpose()
        }
    }
}

pub async fn update_session(
    db: &Db,
    doc: &RegisterSessionDocument,
) -> AppResult<Option<RegisterSessionDocument>> {
    let now = BsonDateTime::now();
    match db {
        Db::Mongo(mongo_db) => {
            let mut update_doc = doc.clone();
            update_doc.updated_at = now;
            update_doc.version += 1;

            let filter = doc! { "key": &doc.key, "version": doc.version };
            let res = sessions_col(mongo_db).replace_one(filter, &update_doc).await?;
            if res.matched_count == 0 {
                return Ok(None);
            }
            Ok(Some(update_doc))
        }
        Db::Sqlite(pool) => {
            let opened_at_iso = bson_to_iso(&doc.opened_at);
            let closed_at_iso = doc.closed_at.as_ref().map(bson_to_iso);
            let reopened_at_iso = doc.reopened_at.as_ref().map(bson_to_iso);
            let updated_at_iso = bson_to_iso(&now);
            let opening_denom_json = doc
                .opening_denominations
                .as_ref()
                .map(|v| serde_json::to_string(v).unwrap_or_default());
            let closing_denom_json = doc
                .closing_denominations
                .as_ref()
                .map(|v| serde_json::to_string(v).unwrap_or_default());

            let res = sqlx::query(
                r#"
                UPDATE register_sessions SET
                    status = ?,
                    opened_at = ?,
                    opening_float_cents = ?,
                    opening_denominations = ?,
                    opening_note = ?,
                    expected_opening_float_cents = ?,
                    opening_discrepancy_cents = ?,
                    opening_discrepancy_reason = ?,
                    opening_approved_by_id = ?,
                    opening_approved_by_name = ?,
                    closed_at = ?,
                    closing_cashier_id = ?,
                    closing_cashier_name_snapshot = ?,
                    expected_cash_cents = ?,
                    counted_cash_cents = ?,
                    cash_variance_cents = ?,
                    closing_denominations = ?,
                    closing_note = ?,
                    closing_float_cents = ?,
                    cash_drop_to_safe_cents = ?,
                    variance_reason = ?,
                    variance_approved_by_id = ?,
                    variance_approved_by_name = ?,
                    reopened_at = ?,
                    reopened_by_id = ?,
                    reopened_by_name = ?,
                    reopened_reason = ?,
                    total_cash_sales_cents = ?,
                    total_card_sales_cents = ?,
                    total_online_sales_cents = ?,
                    total_credit_sales_cents = ?,
                    total_cash_refunds_cents = ?,
                    total_pay_in_cents = ?,
                    total_pay_out_cents = ?,
                    total_owner_withdrawals_cents = ?,
                    total_cash_drops_cents = ?,
                    has_late_movements = ?,
                    version = version + 1,
                    updated_at = ?
                WHERE key = ? AND version = ?
                "#
            )
            .bind(doc.status.as_str())
            .bind(&opened_at_iso)
            .bind(doc.opening_float_cents)
            .bind(&opening_denom_json)
            .bind(&doc.opening_note)
            .bind(doc.expected_opening_float_cents)
            .bind(doc.opening_discrepancy_cents)
            .bind(&doc.opening_discrepancy_reason)
            .bind(&doc.opening_approved_by_id)
            .bind(&doc.opening_approved_by_name)
            .bind(&closed_at_iso)
            .bind(&doc.closing_cashier_id)
            .bind(&doc.closing_cashier_name_snapshot)
            .bind(doc.expected_cash_cents)
            .bind(doc.counted_cash_cents)
            .bind(doc.cash_variance_cents)
            .bind(&closing_denom_json)
            .bind(&doc.closing_note)
            .bind(doc.closing_float_cents)
            .bind(doc.cash_drop_to_safe_cents)
            .bind(&doc.variance_reason)
            .bind(&doc.variance_approved_by_id)
            .bind(&doc.variance_approved_by_name)
            .bind(&reopened_at_iso)
            .bind(&doc.reopened_by_id)
            .bind(&doc.reopened_by_name)
            .bind(&doc.reopened_reason)
            .bind(doc.total_cash_sales_cents)
            .bind(doc.total_card_sales_cents)
            .bind(doc.total_online_sales_cents)
            .bind(doc.total_credit_sales_cents)
            .bind(doc.total_cash_refunds_cents)
            .bind(doc.total_pay_in_cents)
            .bind(doc.total_pay_out_cents)
            .bind(doc.total_owner_withdrawals_cents)
            .bind(doc.total_cash_drops_cents)
            .bind(if doc.has_late_movements { 1 } else { 0 })
            .bind(&updated_at_iso)
            .bind(&doc.key)
            .bind(doc.version)
            .execute(pool)
            .await?;

            if res.rows_affected() == 0 {
                return Ok(None);
            }

            find_session_by_key(db, &doc.key).await
        }
    }
}

pub async fn increment_session_sales(
    db: &Db,
    session_key: &str,
    cash_delta: i64,
    card_delta: i64,
    online_delta: i64,
    credit_delta: i64,
) -> AppResult<()> {
    let now = BsonDateTime::now();
    match db {
        Db::Mongo(mongo_db) => {
            sessions_col(mongo_db)
                .update_one(
                    doc! { "key": session_key },
                    doc! {
                        "$inc": {
                            "total_cash_sales_cents": cash_delta,
                            "total_card_sales_cents": card_delta,
                            "total_online_sales_cents": online_delta,
                            "total_credit_sales_cents": credit_delta,
                            "version": 1,
                        },
                        "$set": { "updated_at": now },
                    },
                )
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            let updated_at_iso = bson_to_iso(&now);
            sqlx::query(
                r#"
                UPDATE register_sessions SET
                    total_cash_sales_cents = total_cash_sales_cents + ?,
                    total_card_sales_cents = total_card_sales_cents + ?,
                    total_online_sales_cents = total_online_sales_cents + ?,
                    total_credit_sales_cents = total_credit_sales_cents + ?,
                    version = version + 1,
                    updated_at = ?
                WHERE key = ?
                "#
            )
            .bind(cash_delta)
            .bind(card_delta)
            .bind(online_delta)
            .bind(credit_delta)
            .bind(&updated_at_iso)
            .bind(session_key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}

pub async fn increment_session_refunds(
    db: &Db,
    session_key: &str,
    refund_cash_delta: i64,
) -> AppResult<()> {
    let now = BsonDateTime::now();
    match db {
        Db::Mongo(mongo_db) => {
            sessions_col(mongo_db)
                .update_one(
                    doc! { "key": session_key },
                    doc! {
                        "$inc": {
                            "total_cash_refunds_cents": refund_cash_delta,
                            "version": 1,
                        },
                        "$set": { "updated_at": now },
                    },
                )
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            let updated_at_iso = bson_to_iso(&now);
            sqlx::query(
                r#"
                UPDATE register_sessions SET
                    total_cash_refunds_cents = total_cash_refunds_cents + ?,
                    version = version + 1,
                    updated_at = ?
                WHERE key = ?
                "#
            )
            .bind(refund_cash_delta)
            .bind(&updated_at_iso)
            .bind(session_key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}

pub async fn increment_session_movements(
    db: &Db,
    session_key: &str,
    movement_type: CashMovementType,
    amount_cents: i64,
) -> AppResult<()> {
    let now = BsonDateTime::now();
    let col_name = match movement_type {
        CashMovementType::PayIn => "total_pay_in_cents",
        CashMovementType::PayOutExpense | CashMovementType::PayOutSupplier => "total_pay_out_cents",
        CashMovementType::OwnerWithdrawal => "total_owner_withdrawals_cents",
        CashMovementType::CashDrop => "total_cash_drops_cents",
        CashMovementType::DrawerOpenNoSale => return Ok(()),
    };

    match db {
        Db::Mongo(mongo_db) => {
            sessions_col(mongo_db)
                .update_one(
                    doc! { "key": session_key },
                    doc! {
                        "$inc": { col_name: amount_cents, "version": 1 },
                        "$set": { "updated_at": now },
                    },
                )
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            let updated_at_iso = bson_to_iso(&now);
            let sql = format!(
                r#"
                UPDATE register_sessions SET
                    {} = {} + ?,
                    version = version + 1,
                    updated_at = ?
                WHERE key = ?
                "#,
                col_name, col_name
            );
            sqlx::query(&sql)
                .bind(amount_cents)
                .bind(&updated_at_iso)
                .bind(session_key)
                .execute(pool)
                .await?;
            Ok(())
        }
    }
}

pub async fn list_sessions(
    db: &Db,
    query: &SessionListQuery,
) -> AppResult<(Vec<RegisterSessionDocument>, u64)> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).min(100);
    let skip = (page - 1) * limit;

    match db {
        Db::Mongo(mongo_db) => {
            let mut filter = doc! {};
            if let Some(terminal_id) = &query.terminal_id {
                filter.insert("terminal_id", terminal_id);
            }
            if let Some(cashier_id) = &query.cashier_id {
                filter.insert("cashier_id", cashier_id);
            }
            if let Some(status) = &query.status {
                filter.insert("status", status);
            }
            if let Some(from) = &query.from {
                filter.insert("opened_at", doc! { "$gte": from });
            }
            if let Some(to) = &query.to {
                filter.entry("opened_at".to_string()).or_insert_with(|| doc! {}.into());
                if let Ok(d) = filter.get_document_mut("opened_at") {
                    d.insert("$lte", to);
                }
            }

            let total = sessions_col(mongo_db).count_documents(filter.clone()).await?;
            let mut cursor = sessions_col(mongo_db)
                .find(filter)
                .sort(doc! { "opened_at": -1 })
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

            if let Some(terminal_id) = &query.terminal_id {
                conditions.push("terminal_id = ?".to_string());
                bindings.push(terminal_id.clone());
            }
            if let Some(cashier_id) = &query.cashier_id {
                conditions.push("cashier_id = ?".to_string());
                bindings.push(cashier_id.clone());
            }
            if let Some(status) = &query.status {
                conditions.push("status = ?".to_string());
                bindings.push(status.clone());
            }
            if let Some(from) = &query.from {
                conditions.push("opened_at >= ?".to_string());
                bindings.push(from.clone());
            }
            if let Some(to) = &query.to {
                conditions.push("opened_at <= ?".to_string());
                bindings.push(to.clone());
            }

            let where_clause = conditions.join(" AND ");
            let count_sql = format!("SELECT COUNT(*) as total FROM register_sessions WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let count_row = count_query.fetch_one(pool).await?;
            let total: i64 = count_row.get("total");

            let select_sql = format!(
                "SELECT * FROM register_sessions WHERE {where_clause} ORDER BY opened_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            select_query = select_query.bind(limit as i64).bind(skip as i64);

            let rows = select_query.fetch_all(pool).await?;
            let mut results = Vec::with_capacity(rows.len());
            for r in rows {
                results.push(session_from_sqlite_row(&r)?);
            }
            Ok((results, total as u64))
        }
    }
}
