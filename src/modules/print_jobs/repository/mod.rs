// Mongo and SQLite access for the `print_jobs` collection. Same never-interpret-a-
// miss-as-an-error convention as every other repository in this codebase —
// `service` decides what a missing row means.

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, now_utc_iso, to_bson_datetime},
    },
    core::error::AppResult,
    modules::print_jobs::model::PrintJobDocument,
};

fn print_jobs(db: &TenantDatabase) -> ScopedCollection<PrintJobDocument> {
    db.collection("print_jobs")
}

fn row_to_print_job_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<PrintJobDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let quantity_int: i64 = row.get("quantity");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(PrintJobDocument {
        id,
        key: row.get("key"),
        ticket_number: row.get("ticket_number"),
        customer_key: row.get("customer_key"),
        customer_name: row.get("customer_name"),
        customer_phone: row.get("customer_phone"),
        job_type: row.get("job_type"),
        quantity: quantity_int as i32,
        promised_ready_at: row.get("promised_ready_at"),
        status: row.get("status"),
        estimated_cost_cents: row.get("estimated_cost_cents"),
        material_cost_cents: row.get("material_cost_cents"),
        assigned_employee_id: row.get("assigned_employee_id"),
        assigned_employee_name: row.get("assigned_employee_name"),
        split_type: row.get("split_type"),
        split_value: row.get("split_value"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}

fn build_print_job_where_clause(filter: &Document) -> (String, Vec<String>) {
    let mut conditions = vec!["deleted_at IS NULL".to_string()];
    let mut bindings = Vec::new();

    fn process_clause(doc: &Document, conditions: &mut Vec<String>, bindings: &mut Vec<String>) {
        if let Ok(key) = doc.get_str("key") {
            conditions.push("key = ?".to_string());
            bindings.push(key.to_string());
        }
        if let Ok(status) = doc.get_str("status") {
            conditions.push("status = ?".to_string());
            bindings.push(status.to_string());
        }
        if let Ok(customer_key) = doc.get_str("customer_key") {
            conditions.push("customer_key = ?".to_string());
            bindings.push(customer_key.to_string());
        }
        if let Ok(created_at_doc) = doc.get_document("created_at") {
            if let Ok(gte) = created_at_doc.get_datetime("$gte") {
                conditions.push("created_at >= ?".to_string());
                bindings.push(bson_to_iso(gte));
            }
            if let Ok(lt) = created_at_doc.get_datetime("$lt") {
                conditions.push("created_at < ?".to_string());
                bindings.push(bson_to_iso(lt));
            }
        }
        if let Ok(or_arr) = doc.get_array("$or") {
            let mut or_parts = Vec::new();
            for or_item in or_arr {
                if let Some(or_doc) = or_item.as_document() {
                    if let Ok(tick_doc) = or_doc.get_document("ticket_number") {
                        if let Ok(regex) = tick_doc.get_str("$regex") {
                            let clean = regex.trim_start_matches('^').trim_end_matches('$');
                            or_parts.push("ticket_number LIKE ?".to_string());
                            bindings.push(format!("%{clean}%"));
                        }
                    } else if let Ok(tick_exact) = or_doc.get_str("ticket_number") {
                        or_parts.push("ticket_number = ?".to_string());
                        bindings.push(tick_exact.to_string());
                    }

                    if let Ok(cname) = or_doc.get_document("customer_name")
                        && let Ok(regex) = cname.get_str("$regex")
                    {
                        let clean = regex.trim_start_matches('^').trim_end_matches('$');
                        or_parts.push("customer_name LIKE ?".to_string());
                        bindings.push(format!("%{clean}%"));
                    }

                    if let Ok(cphone) = or_doc.get_document("customer_phone")
                        && let Ok(regex) = cphone.get_str("$regex")
                    {
                        let clean = regex.trim_start_matches('^').trim_end_matches('$');
                        or_parts.push("customer_phone LIKE ?".to_string());
                        bindings.push(format!("%{clean}%"));
                    }

                    if let Ok(jt) = or_doc.get_document("job_type")
                        && let Ok(regex) = jt.get_str("$regex")
                    {
                        let clean = regex.trim_start_matches('^').trim_end_matches('$');
                        or_parts.push("job_type LIKE ?".to_string());
                        bindings.push(format!("%{clean}%"));
                    }
                }
            }
            if !or_parts.is_empty() {
                conditions.push(format!("({})", or_parts.join(" OR ")));
            }
        }
    }

    if let Ok(and_arr) = filter.get_array("$and") {
        for item in and_arr {
            if let Some(doc) = item.as_document() {
                process_clause(doc, &mut conditions, &mut bindings);
            }
        }
    } else {
        process_clause(filter, &mut conditions, &mut bindings);
    }

    (conditions.join(" AND "), bindings)
}

pub(crate) async fn find_by_id(db: &Db, id: ObjectId) -> AppResult<Option<PrintJobDocument>> {
    match db {
        Db::Mongo(db) => Ok(print_jobs(db)
            .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM print_jobs WHERE id = ? AND deleted_at IS NULL")
                    .bind(id.to_hex())
                    .fetch_optional(pool)
                    .await?;

            row_opt.map(|row| row_to_print_job_doc(&row)).transpose()
        }
    }
}

pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<PrintJobDocument>> {
    match db {
        Db::Mongo(db) => Ok(print_jobs(db)
            .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row_opt =
                sqlx::query("SELECT * FROM print_jobs WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;

            row_opt.map(|row| row_to_print_job_doc(&row)).transpose()
        }
    }
}

pub(crate) async fn find_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<PrintJobDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    find_by_key(db, id_or_key).await
}

pub(crate) async fn insert(db: &Db, mut document: PrintJobDocument) -> AppResult<PrintJobDocument> {
    match db {
        Db::Mongo(db) => {
            let result = print_jobs(db).insert_one(&document).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);
            let deleted_at_iso = document.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO print_jobs (
                    key, id, ticket_number, customer_key, customer_name, customer_phone,
                    job_type, quantity, promised_ready_at, status, estimated_cost_cents,
                    material_cost_cents, assigned_employee_id, assigned_employee_name,
                    split_type, split_value, version, created_at, updated_at, deleted_at,
                    updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.ticket_number)
            .bind(&document.customer_key)
            .bind(&document.customer_name)
            .bind(&document.customer_phone)
            .bind(&document.job_type)
            .bind(document.quantity as i64)
            .bind(&document.promised_ready_at)
            .bind(&document.status)
            .bind(document.estimated_cost_cents)
            .bind(document.material_cost_cents)
            .bind(&document.assigned_employee_id)
            .bind(&document.assigned_employee_name)
            .bind(&document.split_type)
            .bind(document.split_value)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .bind(&deleted_at_iso)
            .bind(&document.updated_by_device)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

pub(crate) async fn update(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<PrintJobDocument>> {
    match db {
        Db::Mongo(db) => Ok(print_jobs(db)
            .find_one_and_update(
                doc! { "_id": id, "deleted_at": { "$exists": false } },
                doc! { "$set": set_doc, "$inc": { "version": 1 } },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_hex = id.to_hex();
            let mut assignments = vec![
                "version = version + 1".to_string(),
                "updated_at = ?".to_string(),
            ];
            let now_iso = now_utc_iso();
            let mut bindings: Vec<Option<String>> = vec![Some(now_iso)];

            for (k, v) in set_doc.iter() {
                if k == "updated_at" {
                    continue;
                }
                match v {
                    mongodb::bson::Bson::Null => {
                        assignments.push(format!("{k} = NULL"));
                    }
                    mongodb::bson::Bson::String(s) => {
                        assignments.push(format!("{k} = ?"));
                        bindings.push(Some(s.clone()));
                    }
                    mongodb::bson::Bson::Int32(i) => {
                        assignments.push(format!("{k} = ?"));
                        bindings.push(Some(i.to_string()));
                    }
                    mongodb::bson::Bson::Int64(i) => {
                        assignments.push(format!("{k} = ?"));
                        bindings.push(Some(i.to_string()));
                    }
                    mongodb::bson::Bson::Double(f) => {
                        assignments.push(format!("{k} = ?"));
                        bindings.push(Some(f.to_string()));
                    }
                    _ => {}
                }
            }

            let sql = format!(
                "UPDATE print_jobs SET {} WHERE id = ? AND deleted_at IS NULL",
                assignments.join(", ")
            );
            let mut query = sqlx::query(&sql);
            for b in bindings {
                query = query.bind(b);
            }
            let res = query.bind(&id_hex).execute(pool).await?;

            if res.rows_affected() == 0 {
                return Ok(None);
            }

            find_by_id(db, id).await
        }
    }
}

/// Narrow status-only mutator, looked up by `key` — used by
/// `service::mark_delivered`, the cross-module hook `billing`'s
/// complete-sale flow calls.
pub(crate) async fn set_status_by_key(
    db: &Db,
    key: &str,
    status: &str,
) -> AppResult<Option<PrintJobDocument>> {
    match db {
        Db::Mongo(db) => Ok(print_jobs(db)
            .find_one_and_update(
                doc! { "key": key, "deleted_at": { "$exists": false } },
                doc! {
                    "$set": { "status": status, "updated_at": BsonDateTime::now() },
                    "$inc": { "version": 1 },
                },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let now_iso = now_utc_iso();
            let res = sqlx::query(
                "UPDATE print_jobs SET status = ?, version = version + 1, updated_at = ? WHERE key = ? AND deleted_at IS NULL",
            )
            .bind(status)
            .bind(&now_iso)
            .bind(key)
            .execute(pool)
            .await?;

            if res.rows_affected() == 0 {
                return Ok(None);
            }

            find_by_key(db, key).await
        }
    }
}

pub(crate) async fn delete(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<PrintJobDocument>> {
    match db {
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            let mut set_doc = doc! { "deleted_at": now, "updated_at": now };
            if let Some(device) = device_id {
                set_doc.insert("updated_by_device", device);
            }
            Ok(print_jobs(db)
                .find_one_and_update(
                    doc! { "_id": id, "deleted_at": { "$exists": false } },
                    doc! { "$set": set_doc, "$inc": { "version": 1 } },
                )
                .return_document(ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let now_iso = now_utc_iso();
            let id_hex = id.to_hex();
            let res = sqlx::query(
                "UPDATE print_jobs SET deleted_at = ?, updated_at = ?, updated_by_device = ?, version = version + 1 WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&now_iso)
            .bind(&now_iso)
            .bind(device_id)
            .bind(&id_hex)
            .execute(pool)
            .await?;

            if res.rows_affected() == 0 {
                return Ok(None);
            }

            let row_opt = sqlx::query("SELECT * FROM print_jobs WHERE id = ?")
                .bind(&id_hex)
                .fetch_optional(pool)
                .await?;

            row_opt.map(|row| row_to_print_job_doc(&row)).transpose()
        }
    }
}

pub(crate) async fn list(
    db: &Db,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<PrintJobDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let mut effective_filter = filter;
            if !effective_filter.contains_key("deleted_at") {
                effective_filter.insert("deleted_at", doc! { "$exists": false });
            }

            let total = print_jobs(db)
                .count_documents(effective_filter.clone())
                .await?;

            let mut cursor = print_jobs(db)
                .find(effective_filter)
                .sort(doc! { "updated_at": -1 })
                .skip(skip)
                .limit(limit as i64)
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok((items, total))
        }
        Db::Sqlite(pool) => {
            let (where_clause, bindings) = build_print_job_where_clause(&filter);

            let count_sql =
                format!("SELECT COUNT(*) as total FROM print_jobs WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let count_row = count_query.fetch_one(pool).await?;
            let total: i64 = count_row.get("total");

            let select_sql = format!(
                "SELECT * FROM print_jobs WHERE {where_clause} ORDER BY updated_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            select_query = select_query.bind(limit as i64).bind(skip as i64);

            let rows = select_query.fetch_all(pool).await?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(row_to_print_job_doc(&row)?);
            }
            Ok((items, total as u64))
        }
    }
}

/// Everything `service::get_print_job_stats` needs from `print_jobs`,
/// computed in one aggregation round-trip.
pub(crate) struct StatsAggregateResult {
    /// Count of non-deleted jobs created in `[today_start, today_end)`.
    pub today_job_count: u64,
    /// Sum of `estimated_cost_cents` for those same jobs.
    pub today_revenue_cents: i64,
    /// Count of ALL non-deleted jobs (any date) whose status is neither
    /// "delivered" nor "cancelled".
    pub pending_job_count: u64,
}

/// Extracts a scalar `i64` field pushed by a `$group` facet branch, out of
/// that branch's array-of-one-document shape (`[{ "_id": null, field: N }]`,
/// empty array if the branch matched nothing).
fn i64_from_facet_branch(result: &Document, branch: &str, field: &str) -> i64 {
    result
        .get_array(branch)
        .ok()
        .and_then(|arr| arr.first())
        .and_then(|val| val.as_document())
        .and_then(|doc| {
            doc.get_i64(field)
                .ok()
                .or_else(|| doc.get_i32(field).ok().map(i64::from))
        })
        .unwrap_or(0)
}

/// Runs a single `$facet` aggregation over `print_jobs` to compute the Print
/// Jobs screen's KPI cards in one database round-trip: today's ticket count
/// + revenue sum, and the all-time open-ticket count.
pub(crate) async fn aggregate_stats(
    db: &Db,
    today_start: DateTime<Utc>,
    today_end: DateTime<Utc>,
) -> AppResult<StatsAggregateResult> {
    match db {
        Db::Mongo(db) => {
            let pipeline = vec![doc! {
                "$facet": {
                    "today": [
                        {
                            "$match": {
                                "deleted_at": { "$exists": false },
                                "created_at": {
                                    "$gte": BsonDateTime::from_chrono(today_start),
                                    "$lt": BsonDateTime::from_chrono(today_end),
                                }
                            }
                        },
                        {
                            "$group": {
                                "_id": null,
                                "sum": { "$sum": "$estimated_cost_cents" },
                                "count": { "$sum": 1 },
                            }
                        }
                    ],
                    "pending": [
                        {
                            "$match": {
                                "deleted_at": { "$exists": false },
                                "status": { "$nin": ["delivered", "cancelled"] },
                            }
                        },
                        { "$count": "count" }
                    ],
                }
            }];

            let mut cursor = print_jobs(db).aggregate(pipeline).await?;
            let Some(result) = cursor.try_next().await? else {
                return Ok(StatsAggregateResult {
                    today_job_count: 0,
                    today_revenue_cents: 0,
                    pending_job_count: 0,
                });
            };

            let today_job_count = i64_from_facet_branch(&result, "today", "count") as u64;
            let today_revenue_cents = i64_from_facet_branch(&result, "today", "sum");
            let pending_job_count = i64_from_facet_branch(&result, "pending", "count") as u64;

            Ok(StatsAggregateResult {
                today_job_count,
                today_revenue_cents,
                pending_job_count,
            })
        }
        Db::Sqlite(pool) => {
            let start_iso = today_start.to_rfc3339();
            let end_iso = today_end.to_rfc3339();

            let row = sqlx::query(
                r#"
                SELECT
                    COUNT(CASE WHEN created_at >= ? AND created_at < ? THEN 1 ELSE NULL END) as today_job_count,
                    COALESCE(SUM(CASE WHEN created_at >= ? AND created_at < ? THEN estimated_cost_cents ELSE 0 END), 0) as today_revenue_cents,
                    COUNT(CASE WHEN status NOT IN ('delivered', 'cancelled') THEN 1 ELSE NULL END) as pending_job_count
                FROM print_jobs
                WHERE deleted_at IS NULL
                "#,
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_one(pool)
            .await?;

            let today_job_count: i64 = row.get("today_job_count");
            let today_revenue_cents: i64 = row.get("today_revenue_cents");
            let pending_job_count: i64 = row.get("pending_job_count");

            Ok(StatsAggregateResult {
                today_job_count: today_job_count as u64,
                today_revenue_cents,
                pending_job_count: pending_job_count as u64,
            })
        }
    }
}
