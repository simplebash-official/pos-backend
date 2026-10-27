// Mongo and SQLite access for the `quotations` collection/table. Same
// never-interpret-a-miss-as-an-error convention as every other repository —
// `service` decides what a missing row means.

use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, now_utc_iso, to_bson_datetime},
        tenant_db::{ScopedCollection, TenantDatabase},
    },
    core::{
        error::{AppError, AppResult},
        utils::build_bson_regex,
    },
    domain::quotations::{STATUS_ACTIVE, STATUS_CANCELLED, STATUS_FULFILLED},
    modules::quotations::model::QuotationDocument,
};

fn quotations(db: &TenantDatabase) -> ScopedCollection<QuotationDocument> {
    db.collection("quotations")
}

/// What `GET /quotations` filters on. `status` is one of the three list
/// views (`active`, `expired`, `cancelled`) and is validated by `service`;
/// `today` is `YYYY-MM-DD` and is what separates active from expired.
pub(crate) struct ListFilter {
    pub search: Option<String>,
    pub status: Option<String>,
    pub today: String,
}

fn row_to_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<QuotationDocument> {
    let id_str: String = row.get("id");
    let items_json: String = row.get("items");
    let snapshot_json: String = row.get("shop_profile_snapshot");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    let deleted_at: Option<String> = row.get("deleted_at");
    let fulfilled_by_invoice_key: Option<String> =
        row.try_get("fulfilled_by_invoice_key").ok().flatten();
    let fulfilled_at: Option<String> = row.try_get("fulfilled_at").ok().flatten();
    let has_out_of_stock_items: bool = row.try_get("has_out_of_stock_items").unwrap_or(false);

    Ok(QuotationDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: row.get("key"),
        quotation_number: row.get("quotation_number"),
        status: row.get("status"),
        has_out_of_stock_items,
        fulfilled_by_invoice_key,
        fulfilled_at: fulfilled_at.map(|s| to_bson_datetime(&s)),
        customer_key: row.get("customer_key"),
        customer_name: row.get("customer_name"),
        customer_phone: row.get("customer_phone"),
        customer_address: row.get("customer_address"),
        items: serde_json::from_str(&items_json)
            .map_err(|e| AppError::internal(format!("Stored quotation items are corrupt: {e}")))?,
        subtotal_cents: row.get("subtotal_cents"),
        discount_type: row.get("discount_type"),
        discount_value: row.get("discount_value"),
        discount_cents: row.get("discount_cents"),
        total_cents: row.get("total_cents"),
        valid_until: row.get("valid_until"),
        notes: row.get("notes"),
        terms: row.get("terms"),
        prepared_by: row.get("prepared_by"),
        shop_profile_snapshot: serde_json::from_str(&snapshot_json).map_err(|e| {
            AppError::internal(format!("Stored quotation shop profile is corrupt: {e}"))
        })?,
        created_at: to_bson_datetime(&created_at),
        updated_at: to_bson_datetime(&updated_at),
        version: row.get("version"),
        deleted_at: deleted_at.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}

pub(crate) async fn find_by_id(db: &Db, id: ObjectId) -> AppResult<Option<QuotationDocument>> {
    match db {
        Db::Mongo(db) => Ok(quotations(db).find_one(doc! { "_id": id }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM quotations WHERE id = ?")
                .bind(id.to_hex())
                .fetch_optional(pool)
                .await?;
            row.map(|r| row_to_doc(&r)).transpose()
        }
    }
}

pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<QuotationDocument>> {
    match db {
        Db::Mongo(db) => Ok(quotations(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM quotations WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.map(|r| row_to_doc(&r)).transpose()
        }
    }
}

pub(crate) async fn find_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<QuotationDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(found) = find_by_id(db, object_id).await?
    {
        return Ok(Some(found));
    }
    find_by_key(db, id_or_key).await
}

pub(crate) async fn insert(
    db: &Db,
    mut document: QuotationDocument,
) -> AppResult<QuotationDocument> {
    match db {
        Db::Mongo(db) => {
            let result = quotations(db).insert_one(&document).await?;
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
            let items_json = serde_json::to_string(&document.items)
                .map_err(|e| AppError::internal(format!("Could not store quotation items: {e}")))?;
            let snapshot_json =
                serde_json::to_string(&document.shop_profile_snapshot).map_err(|e| {
                    AppError::internal(format!("Could not store quotation shop profile: {e}"))
                })?;

            sqlx::query(
                r#"
                INSERT INTO quotations (
                    key, id, quotation_number, status, has_out_of_stock_items, customer_key, customer_name,
                    customer_phone, customer_address, items, subtotal_cents, discount_type,
                    discount_value, discount_cents, total_cents, valid_until, notes, terms,
                    prepared_by, shop_profile_snapshot, version, created_at, updated_at,
                    deleted_at, updated_by_device, fulfilled_by_invoice_key, fulfilled_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.quotation_number)
            .bind(&document.status)
            .bind(if document.has_out_of_stock_items { 1i32 } else { 0i32 })
            .bind(&document.customer_key)
            .bind(&document.customer_name)
            .bind(&document.customer_phone)
            .bind(&document.customer_address)
            .bind(&items_json)
            .bind(document.subtotal_cents)
            .bind(&document.discount_type)
            .bind(document.discount_value)
            .bind(document.discount_cents)
            .bind(document.total_cents)
            .bind(&document.valid_until)
            .bind(&document.notes)
            .bind(&document.terms)
            .bind(&document.prepared_by)
            .bind(&snapshot_json)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .bind(document.deleted_at.as_ref().map(bson_to_iso))
            .bind(&document.updated_by_device)
            .bind(&document.fulfilled_by_invoice_key)
            .bind(document.fulfilled_at.as_ref().map(bson_to_iso))
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

/// Marks an `active` quotation `cancelled`, bumping its sync `version` and
/// recording which device did it. Returns `None` when the row is missing *or*
/// already not active, so two racing cancels cannot both win;
/// `service::cancel_quotation` tells the two cases apart beforehand.
pub(crate) async fn cancel(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<QuotationDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut set_doc = doc! {
                "status": STATUS_CANCELLED,
                "updated_at": BsonDateTime::now(),
            };
            if let Some(device) = device_id {
                set_doc.insert("updated_by_device", device);
            }
            Ok(quotations(db)
                .find_one_and_update(
                    doc! { "_id": id, "status": STATUS_ACTIVE },
                    doc! { "$set": set_doc, "$inc": { "version": 1 } },
                )
                .return_document(mongodb::options::ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let res = sqlx::query(
                "UPDATE quotations SET status = ?, updated_at = ?, updated_by_device = ?, \
                 version = version + 1 WHERE id = ? AND status = ?",
            )
            .bind(STATUS_CANCELLED)
            .bind(now_utc_iso())
            .bind(&device_id)
            .bind(id.to_hex())
            .bind(STATUS_ACTIVE)
            .execute(pool)
            .await?;
            if res.rows_affected() == 0 {
                return Ok(None);
            }
            find_by_id(db, id).await
        }
    }
}

/// Fulfills an active quotation, transitioning status to `fulfilled` and linking
/// to the invoice that converted it.
pub(crate) async fn fulfill(
    db: &Db,
    id: ObjectId,
    invoice_key: &str,
    device_id: Option<String>,
) -> AppResult<Option<QuotationDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut set_doc = doc! {
                "status": STATUS_FULFILLED,
                "fulfilled_by_invoice_key": invoice_key,
                "fulfilled_at": BsonDateTime::now(),
                "updated_at": BsonDateTime::now(),
            };
            if let Some(device) = device_id {
                set_doc.insert("updated_by_device", device);
            }
            Ok(quotations(db)
                .find_one_and_update(
                    doc! { "_id": id, "status": STATUS_ACTIVE },
                    doc! { "$set": set_doc, "$inc": { "version": 1 } },
                )
                .return_document(mongodb::options::ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let res = sqlx::query(
                "UPDATE quotations SET status = ?, fulfilled_by_invoice_key = ?, fulfilled_at = ?, \
                 updated_at = ?, updated_by_device = ?, version = version + 1 \
                 WHERE id = ? AND status = ?",
            )
            .bind(STATUS_FULFILLED)
            .bind(invoice_key)
            .bind(now_utc_iso())
            .bind(now_utc_iso())
            .bind(&device_id)
            .bind(id.to_hex())
            .bind(STATUS_ACTIVE)
            .execute(pool)
            .await?;
            if res.rows_affected() == 0 {
                return Ok(None);
            }
            find_by_id(db, id).await
        }
    }
}

fn mongo_filter(filter: &ListFilter) -> Document {
    let mut clauses: Vec<Document> = Vec::new();
    if let Some(search) = &filter.search {
        let pattern = build_bson_regex(search);
        clauses.push(doc! { "$or": [
            { "quotation_number": { "$regex": pattern.clone() } },
            { "customer_name": { "$regex": pattern.clone() } },
            { "customer_phone": { "$regex": pattern } },
        ] });
    }
    match filter.status.as_deref() {
        Some("active") => clauses.push(
            doc! { "status": STATUS_ACTIVE, "valid_until": { "$gte": filter.today.as_str() } },
        ),
        Some("expired") => clauses.push(
            doc! { "status": STATUS_ACTIVE, "valid_until": { "$lt": filter.today.as_str() } },
        ),
        Some("fulfilled") => clauses.push(doc! { "status": STATUS_FULFILLED }),
        Some("cancelled") => clauses.push(doc! { "status": STATUS_CANCELLED }),
        _ => {}
    }
    if clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": clauses }
    }
}

fn sqlite_where(filter: &ListFilter) -> (String, Vec<String>) {
    let mut conditions: Vec<String> = vec!["1 = 1".to_string()];
    let mut bindings: Vec<String> = Vec::new();
    if let Some(search) = &filter.search {
        conditions.push(
            "(quotation_number LIKE ? OR customer_name LIKE ? OR customer_phone LIKE ?)"
                .to_string(),
        );
        let like = format!("%{search}%");
        bindings.extend([like.clone(), like.clone(), like]);
    }
    match filter.status.as_deref() {
        Some("active") => {
            conditions.push("status = ? AND valid_until >= ?".to_string());
            bindings.extend([STATUS_ACTIVE.to_string(), filter.today.clone()]);
        }
        Some("expired") => {
            conditions.push("status = ? AND valid_until < ?".to_string());
            bindings.extend([STATUS_ACTIVE.to_string(), filter.today.clone()]);
        }
        Some("fulfilled") => {
            conditions.push("status = ?".to_string());
            bindings.push(STATUS_FULFILLED.to_string());
        }
        Some("cancelled") => {
            conditions.push("status = ?".to_string());
            bindings.push(STATUS_CANCELLED.to_string());
        }
        _ => {}
    }
    (conditions.join(" AND "), bindings)
}

/// One page of quotations, newest first, plus the total matching count.
pub(crate) async fn list(
    db: &Db,
    filter: &ListFilter,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<QuotationDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let query = mongo_filter(filter);
            let total = quotations(db).count_documents(query.clone()).await?;
            let mut cursor = quotations(db)
                .find(query)
                .sort(doc! { "created_at": -1 })
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
            let (where_clause, bindings) = sqlite_where(filter);

            let count_sql =
                format!("SELECT COUNT(*) AS total FROM quotations WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let total: i64 = count_query.fetch_one(pool).await?.get("total");

            let select_sql = format!(
                "SELECT * FROM quotations WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            let rows = select_query
                .bind(limit as i64)
                .bind(skip as i64)
                .fetch_all(pool)
                .await?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(row_to_doc(&row)?);
            }
            Ok((items, total as u64))
        }
    }
}
