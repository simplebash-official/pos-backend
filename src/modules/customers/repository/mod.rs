// Dual-engine (Mongo / SQLite) access for the `customers` collection.
// Functions here return `Option`/`Vec`/counts straight from the driver.

use futures_util::TryStreamExt;
use mongodb::{
    Collection,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
    },
    core::error::AppResult,
    modules::customers::model::CustomerDocument,
};

#[derive(sqlx::FromRow)]
struct CustomerSqliteRow {
    id: String,
    key: String,
    name: String,
    contact_person: Option<String>,
    primary_phone: String,
    secondary_phone: Option<String>,
    email: Option<String>,
    address: Option<String>,
    tags: String,
    notes: Option<String>,
    outstanding_balance_cents: i64,
    total_purchases_cents: i64,
    version: i64,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    updated_by_device: Option<String>,
}

impl CustomerSqliteRow {
    fn into_document(self) -> CustomerDocument {
        let tags: Vec<String> = serde_json::from_str(&self.tags).unwrap_or_default();
        CustomerDocument {
            id: ObjectId::parse_str(&self.id).ok(),
            key: self.key,
            name: self.name,
            contact_person: self.contact_person,
            primary_phone: self.primary_phone,
            secondary_phone: self.secondary_phone,
            email: self.email,
            address: self.address,
            tags,
            notes: self.notes,
            outstanding_balance_cents: self.outstanding_balance_cents,
            total_purchases_cents: self.total_purchases_cents,
            version: self.version,
            created_at: to_bson_datetime(&self.created_at),
            updated_at: to_bson_datetime(&self.updated_at),
            deleted_at: self.deleted_at.as_deref().map(to_bson_datetime),
            updated_by_device: self.updated_by_device,
        }
    }
}

fn customers(db: &mongodb::Database) -> Collection<CustomerDocument> {
    db.collection("customers")
}

pub(crate) async fn find_customer_by_id(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<CustomerDocument>> {
    match db {
        Db::Mongo(db) => Ok(customers(db)
            .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let row = sqlx::query_as::<_, CustomerSqliteRow>(
                "SELECT * FROM customers WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&id_str)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(CustomerSqliteRow::into_document))
        }
    }
}

pub(crate) async fn find_customer_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<CustomerDocument>> {
    match db {
        Db::Mongo(db) => Ok(customers(db)
            .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query_as::<_, CustomerSqliteRow>(
                "SELECT * FROM customers WHERE key = ? AND deleted_at IS NULL",
            )
            .bind(key)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(CustomerSqliteRow::into_document))
        }
    }
}

pub(crate) async fn find_customer_by_id_or_key(
    db: &Db,
    id_or_key: &str,
) -> AppResult<Option<CustomerDocument>> {
    if let Ok(object_id) = ObjectId::parse_str(id_or_key)
        && let Some(doc) = find_customer_by_id(db, object_id).await?
    {
        return Ok(Some(doc));
    }
    find_customer_by_key(db, id_or_key).await
}

#[allow(dead_code)]
pub(crate) async fn find_customers_by_keys(
    db: &Db,
    keys: &[String],
) -> AppResult<Vec<CustomerDocument>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = customers(db)
                .find(doc! { "key": { "$in": keys }, "deleted_at": { "$exists": false } })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let placeholders = keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT * FROM customers WHERE key IN ({placeholders}) AND deleted_at IS NULL"
            );
            let mut query = sqlx::query_as::<_, CustomerSqliteRow>(&sql);
            for key in keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            Ok(rows
                .into_iter()
                .map(CustomerSqliteRow::into_document)
                .collect())
        }
    }
}

pub(crate) async fn insert_customer(
    db: &Db,
    mut document: CustomerDocument,
) -> AppResult<CustomerDocument> {
    match db {
        Db::Mongo(db) => {
            let result = customers(db).insert_one(&document).await?;
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
            let tags_json =
                serde_json::to_string(&document.tags).unwrap_or_else(|_| "[]".to_string());
            let created_at = bson_to_iso(&document.created_at);
            let updated_at = bson_to_iso(&document.updated_at);
            let deleted_at = document.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO customers (
                    key, id, name, contact_person, primary_phone, secondary_phone,
                    email, address, tags, notes, outstanding_balance_cents,
                    total_purchases_cents, version, created_at, updated_at,
                    deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.name)
            .bind(&document.contact_person)
            .bind(&document.primary_phone)
            .bind(&document.secondary_phone)
            .bind(&document.email)
            .bind(&document.address)
            .bind(&tags_json)
            .bind(&document.notes)
            .bind(document.outstanding_balance_cents)
            .bind(document.total_purchases_cents)
            .bind(document.version)
            .bind(&created_at)
            .bind(&updated_at)
            .bind(&deleted_at)
            .bind(&document.updated_by_device)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

pub(crate) async fn update_customer(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<CustomerDocument>> {
    match db {
        Db::Mongo(db) => Ok(customers(db)
            .find_one_and_update(
                doc! { "_id": id, "deleted_at": { "$exists": false } },
                doc! { "$set": set_doc, "$inc": { "version": 1 } },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let existing = find_customer_by_id(db, id).await?;
            let Some(mut cust) = existing else {
                return Ok(None);
            };

            if let Ok(name) = set_doc.get_str("name") {
                cust.name = name.to_string();
            }
            if set_doc.contains_key("contact_person") {
                cust.contact_person = set_doc
                    .get_str("contact_person")
                    .ok()
                    .map(|s| s.to_string());
            }
            if let Ok(phone) = set_doc.get_str("primary_phone") {
                cust.primary_phone = phone.to_string();
            }
            if set_doc.contains_key("secondary_phone") {
                cust.secondary_phone = set_doc
                    .get_str("secondary_phone")
                    .ok()
                    .map(|s| s.to_string());
            }
            if set_doc.contains_key("email") {
                cust.email = set_doc.get_str("email").ok().map(|s| s.to_string());
            }
            if set_doc.contains_key("address") {
                cust.address = set_doc.get_str("address").ok().map(|s| s.to_string());
            }
            if let Ok(tags_arr) = set_doc.get_array("tags") {
                cust.tags = tags_arr
                    .iter()
                    .filter_map(|b| b.as_str().map(str::to_string))
                    .collect();
            }
            if set_doc.contains_key("notes") {
                cust.notes = set_doc.get_str("notes").ok().map(|s| s.to_string());
            }
            if set_doc.contains_key("updated_by_device") {
                cust.updated_by_device = set_doc
                    .get_str("updated_by_device")
                    .ok()
                    .map(|s| s.to_string());
            }
            cust.version += 1;
            cust.updated_at = BsonDateTime::now();

            let tags_json = serde_json::to_string(&cust.tags).unwrap_or_else(|_| "[]".to_string());
            let updated_at_iso = bson_to_iso(&cust.updated_at);

            sqlx::query(
                r#"
                UPDATE customers SET
                    name = ?, contact_person = ?, primary_phone = ?, secondary_phone = ?,
                    email = ?, address = ?, tags = ?, notes = ?, updated_by_device = ?,
                    version = ?, updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                "#,
            )
            .bind(&cust.name)
            .bind(&cust.contact_person)
            .bind(&cust.primary_phone)
            .bind(&cust.secondary_phone)
            .bind(&cust.email)
            .bind(&cust.address)
            .bind(&tags_json)
            .bind(&cust.notes)
            .bind(&cust.updated_by_device)
            .bind(cust.version)
            .bind(&updated_at_iso)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(cust))
        }
    }
}

pub(crate) async fn delete_customer(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<CustomerDocument>> {
    match db {
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            let mut set_doc = doc! {
                "deleted_at": now,
                "updated_at": now,
            };
            if let Some(device) = device_id {
                set_doc.insert("updated_by_device", device);
            }
            Ok(customers(db)
                .find_one_and_update(
                    doc! { "_id": id, "deleted_at": { "$exists": false } },
                    doc! {
                        "$set": set_doc,
                        "$inc": { "version": 1 }
                    },
                )
                .return_document(ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let existing = find_customer_by_id(db, id).await?;
            let Some(mut cust) = existing else {
                return Ok(None);
            };

            let now = BsonDateTime::now();
            cust.deleted_at = Some(now);
            cust.updated_at = now;
            cust.version += 1;
            cust.updated_by_device = device_id.clone();

            let now_iso = bson_to_iso(&now);

            sqlx::query(
                "UPDATE customers SET deleted_at = ?, updated_at = ?, updated_by_device = ?, version = version + 1 WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&now_iso)
            .bind(&now_iso)
            .bind(&device_id)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(cust))
        }
    }
}

pub(crate) async fn list_customers(
    db: &Db,
    filter: Document,
    sort: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<CustomerDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let mut effective_filter = filter;
            if !effective_filter.contains_key("deleted_at") {
                effective_filter.insert("deleted_at", doc! { "$exists": false });
            }

            let total = customers(db)
                .count_documents(effective_filter.clone())
                .await?;

            let mut cursor = customers(db)
                .find(effective_filter)
                .sort(sort)
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
            let rows = sqlx::query_as::<_, CustomerSqliteRow>(
                "SELECT * FROM customers WHERE deleted_at IS NULL",
            )
            .fetch_all(pool)
            .await?;

            let mut items: Vec<CustomerDocument> = rows
                .into_iter()
                .map(CustomerSqliteRow::into_document)
                .collect();

            // Filter in-memory by clauses
            if let Ok(and_arr) = filter.get_array("$and") {
                for clause in and_arr {
                    if let Some(clause_doc) = clause.as_document() {
                        if let Ok(tag_str) = clause_doc.get_str("tags") {
                            items.retain(|c| c.tags.iter().any(|t| t == tag_str));
                        }
                        if let Ok(bal_doc) = clause_doc.get_document("outstanding_balance_cents")
                            && let Ok(gt_val) = bal_doc.get_i64("$gt")
                        {
                            items.retain(|c| c.outstanding_balance_cents > gt_val);
                        }
                        if let Ok(or_arr) = clause_doc.get_array("$or") {
                            let mut pattern = String::new();
                            for cond in or_arr {
                                if let Some(cond_doc) = cond.as_document() {
                                    for key in ["name", "primary_phone", "secondary_phone", "email"]
                                    {
                                        if let Ok(field_doc) = cond_doc.get_document(key)
                                            && let Ok(regex) = field_doc.get_str("$regex")
                                        {
                                            pattern = regex.to_lowercase();
                                            break;
                                        }
                                    }
                                }
                                if !pattern.is_empty() {
                                    break;
                                }
                            }
                            if !pattern.is_empty() {
                                items.retain(|c| {
                                    c.name.to_lowercase().contains(&pattern)
                                        || c.primary_phone.to_lowercase().contains(&pattern)
                                        || c.secondary_phone
                                            .as_ref()
                                            .map(|p| p.to_lowercase().contains(&pattern))
                                            .unwrap_or(false)
                                        || c.email
                                            .as_ref()
                                            .map(|e| e.to_lowercase().contains(&pattern))
                                            .unwrap_or(false)
                                });
                            }
                        }
                    }
                }
            }

            // Sort
            if let Ok(dir) = sort.get_i32("name") {
                if dir == -1 {
                    items.sort_by(|a, b| b.name.cmp(&a.name));
                } else {
                    items.sort_by(|a, b| a.name.cmp(&b.name));
                }
            } else if let Ok(dir) = sort.get_i32("created_at") {
                if dir == -1 {
                    items.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                } else {
                    items.sort_by_key(|a| a.created_at);
                }
            } else if let Ok(dir) = sort.get_i32("outstanding_balance_cents") {
                if dir == -1 {
                    items.sort_by(|a, b| {
                        b.outstanding_balance_cents
                            .cmp(&a.outstanding_balance_cents)
                    });
                } else {
                    items.sort_by(|a, b| {
                        a.outstanding_balance_cents
                            .cmp(&b.outstanding_balance_cents)
                    });
                }
            }

            let total = items.len() as u64;
            let paged: Vec<CustomerDocument> = items
                .into_iter()
                .skip(skip as usize)
                .take(limit as usize)
                .collect();

            Ok((paged, total))
        }
    }
}

pub(crate) async fn distinct_tags(db: &Db) -> AppResult<Vec<String>> {
    match db {
        Db::Mongo(db) => {
            let values = customers(db)
                .distinct("tags", doc! { "deleted_at": { "$exists": false } })
                .await?;

            Ok(values
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect())
        }
        Db::Sqlite(pool) => {
            let rows: Vec<String> =
                sqlx::query_scalar("SELECT tags FROM customers WHERE deleted_at IS NULL")
                    .fetch_all(pool)
                    .await?;

            let mut tag_set = std::collections::BTreeSet::new();
            for tags_json in rows {
                if let Ok(tags) = serde_json::from_str::<Vec<String>>(&tags_json) {
                    for tag in tags {
                        tag_set.insert(tag);
                    }
                }
            }
            Ok(tag_set.into_iter().collect())
        }
    }
}

/// Atomically adjusts customer total purchases and outstanding balance via `$inc`.
/// Used by billing/invoices and payments modules.
#[allow(dead_code)]
pub(crate) async fn adjust_customer_financials(
    db: &Db,
    key: &str,
    purchases_delta: i64,
    balance_delta: i64,
) -> AppResult<Option<CustomerDocument>> {
    match db {
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            Ok(customers(db)
                .find_one_and_update(
                    doc! { "key": key, "deleted_at": { "$exists": false } },
                    doc! {
                        "$inc": {
                            "total_purchases_cents": purchases_delta,
                            "outstanding_balance_cents": balance_delta,
                            "version": 1,
                        },
                        "$set": {
                            "updated_at": now,
                        }
                    },
                )
                .return_document(ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let existing = find_customer_by_key(db, key).await?;
            let Some(mut cust) = existing else {
                return Ok(None);
            };

            cust.total_purchases_cents += purchases_delta;
            cust.outstanding_balance_cents += balance_delta;
            cust.version += 1;
            cust.updated_at = BsonDateTime::now();

            let updated_at_iso = bson_to_iso(&cust.updated_at);

            sqlx::query(
                r#"
                UPDATE customers SET
                    total_purchases_cents = total_purchases_cents + ?,
                    outstanding_balance_cents = outstanding_balance_cents + ?,
                    version = version + 1,
                    updated_at = ?
                WHERE key = ? AND deleted_at IS NULL
                "#,
            )
            .bind(purchases_delta)
            .bind(balance_delta)
            .bind(&updated_at_iso)
            .bind(key)
            .execute(pool)
            .await?;

            Ok(Some(cust))
        }
    }
}

pub(crate) struct StatsAggregateResult {
    pub total_customers: u64,
    pub total_balance_due_cents: i64,
    pub active_debtors_count: u64,
}

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

pub(crate) async fn aggregate_stats(db: &Db) -> AppResult<StatsAggregateResult> {
    match db {
        Db::Mongo(db) => {
            let not_deleted = doc! { "deleted_at": { "$exists": false } };
            let pipeline = vec![doc! {
                "$facet": {
                    "total": [
                        { "$match": not_deleted.clone() },
                        { "$count": "count" }
                    ],
                    "debtors": [
                        { "$match": { "$and": [not_deleted, { "outstanding_balance_cents": { "$gt": 0 } }] } },
                        { "$group": { "_id": null, "sum": { "$sum": "$outstanding_balance_cents" }, "count": { "$sum": 1 } } }
                    ],
                }
            }];

            let mut cursor = customers(db).aggregate(pipeline).await?;
            let Some(result) = cursor.try_next().await? else {
                return Ok(StatsAggregateResult {
                    total_customers: 0,
                    total_balance_due_cents: 0,
                    active_debtors_count: 0,
                });
            };

            let total_customers = i64_from_facet_branch(&result, "total", "count") as u64;
            let total_balance_due_cents = i64_from_facet_branch(&result, "debtors", "sum");
            let active_debtors_count = i64_from_facet_branch(&result, "debtors", "count") as u64;

            Ok(StatsAggregateResult {
                total_customers,
                total_balance_due_cents,
                active_debtors_count,
            })
        }
        Db::Sqlite(pool) => {
            #[derive(sqlx::FromRow)]
            struct SqliteStats {
                total_customers: i64,
                total_balance_due_cents: i64,
                active_debtors_count: i64,
            }

            let stats = sqlx::query_as::<_, SqliteStats>(
                r#"
                SELECT
                    COUNT(*) as total_customers,
                    COALESCE(SUM(CASE WHEN outstanding_balance_cents > 0 THEN outstanding_balance_cents ELSE 0 END), 0) as total_balance_due_cents,
                    COUNT(CASE WHEN outstanding_balance_cents > 0 THEN 1 ELSE NULL END) as active_debtors_count
                FROM customers
                WHERE deleted_at IS NULL
                "#,
            )
            .fetch_one(pool)
            .await?;

            Ok(StatsAggregateResult {
                total_customers: stats.total_customers as u64,
                total_balance_due_cents: stats.total_balance_due_cents,
                active_debtors_count: stats.active_debtors_count as u64,
            })
        }
    }
}
