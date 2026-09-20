// Dual-engine (Mongo / SQLite) access for the `employees` collection.
// Functions here return `Option`/`Vec` straight from the driver.

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, now_utc_iso, to_bson_datetime},
    },
    core::error::AppResult,
    domain::employees::{EmployeeRole, EmployeeStatus, SplitType},
    modules::employees::model::EmployeeDocument,
};

#[derive(sqlx::FromRow)]
struct EmployeeSqliteRow {
    id: String,
    key: String,
    name: String,
    phone: String,
    nic_or_id: Option<String>,
    role: String,
    default_split_type: String,
    default_split_value: f64,
    status: String,
    notes: Option<String>,
    version: i64,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    updated_by_device: Option<String>,
}

impl EmployeeSqliteRow {
    fn into_document(self) -> EmployeeDocument {
        EmployeeDocument {
            id: ObjectId::parse_str(&self.id).ok(),
            key: self.key,
            name: self.name,
            phone: self.phone,
            nic_or_id: self.nic_or_id,
            role: self.role.parse().unwrap_or(EmployeeRole::General),
            default_split_type: self
                .default_split_type
                .parse()
                .unwrap_or(SplitType::Percentage),
            default_split_value: self.default_split_value,
            status: self.status.parse().unwrap_or(EmployeeStatus::Active),
            notes: self.notes,
            version: self.version,
            created_at: to_bson_datetime(&self.created_at),
            updated_at: to_bson_datetime(&self.updated_at),
            deleted_at: self.deleted_at.as_deref().map(to_bson_datetime),
            updated_by_device: self.updated_by_device,
        }
    }
}

fn employees(db: &TenantDatabase) -> ScopedCollection<EmployeeDocument> {
    db.collection("employees")
}

pub(crate) async fn find_employee_by_id(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<EmployeeDocument>> {
    match db {
        Db::Mongo(db) => Ok(employees(db)
            .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let row = sqlx::query_as::<_, EmployeeSqliteRow>(
                "SELECT * FROM employees WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&id_str)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(EmployeeSqliteRow::into_document))
        }
    }
}

pub(crate) async fn find_employee_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<EmployeeDocument>> {
    match db {
        Db::Mongo(db) => Ok(employees(db)
            .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query_as::<_, EmployeeSqliteRow>(
                "SELECT * FROM employees WHERE key = ? AND deleted_at IS NULL",
            )
            .bind(key)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(EmployeeSqliteRow::into_document))
        }
    }
}

pub(crate) async fn find_employees_by_keys(
    db: &Db,
    keys: &[String],
) -> AppResult<Vec<EmployeeDocument>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = employees(db)
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
                "SELECT * FROM employees WHERE key IN ({placeholders}) AND deleted_at IS NULL"
            );
            let mut query = sqlx::query_as::<_, EmployeeSqliteRow>(&sql);
            for key in keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            Ok(rows
                .into_iter()
                .map(EmployeeSqliteRow::into_document)
                .collect())
        }
    }
}

pub(crate) async fn touch_employee_by_key(db: &Db, key: &str) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            employees(db)
                .update_one(
                    doc! { "key": key, "deleted_at": { "$exists": false } },
                    doc! {
                        "$set": { "updated_at": BsonDateTime::now() },
                        "$inc": { "version": 1 }
                    },
                )
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            let now = now_utc_iso();
            sqlx::query(
                "UPDATE employees SET updated_at = ?, version = version + 1 WHERE key = ? AND deleted_at IS NULL",
            )
            .bind(now)
            .bind(key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}

pub(crate) async fn insert_employee(
    db: &Db,
    mut document: EmployeeDocument,
) -> AppResult<EmployeeDocument> {
    match db {
        Db::Mongo(db) => {
            let result = employees(db).insert_one(&document).await?;
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
            let created_at = bson_to_iso(&document.created_at);
            let updated_at = bson_to_iso(&document.updated_at);
            let deleted_at = document.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO employees (
                    key, id, name, phone, nic_or_id, role, default_split_type,
                    default_split_value, status, notes, version, created_at,
                    updated_at, deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.name)
            .bind(&document.phone)
            .bind(&document.nic_or_id)
            .bind(document.role.as_str())
            .bind(document.default_split_type.as_str())
            .bind(document.default_split_value)
            .bind(document.status.as_str())
            .bind(&document.notes)
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

pub(crate) async fn update_employee(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<EmployeeDocument>> {
    match db {
        Db::Mongo(db) => Ok(employees(db)
            .find_one_and_update(
                doc! { "_id": id, "deleted_at": { "$exists": false } },
                doc! { "$set": set_doc, "$inc": { "version": 1 } },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let existing = find_employee_by_id(db, id).await?;
            let Some(mut emp) = existing else {
                return Ok(None);
            };

            if let Ok(name) = set_doc.get_str("name") {
                emp.name = name.to_string();
            }
            if let Ok(phone) = set_doc.get_str("phone") {
                emp.phone = phone.to_string();
            }
            if set_doc.contains_key("nic_or_id") {
                emp.nic_or_id = set_doc.get_str("nic_or_id").ok().map(|s| s.to_string());
            }
            if let Ok(role) = set_doc.get_str("role")
                && let Ok(r) = role.parse()
            {
                emp.role = r;
            }
            if let Ok(split_type) = set_doc.get_str("default_split_type")
                && let Ok(st) = split_type.parse()
            {
                emp.default_split_type = st;
            }
            if let Ok(split_value) = set_doc.get_f64("default_split_value") {
                emp.default_split_value = split_value;
            }
            if let Ok(status) = set_doc.get_str("status")
                && let Ok(s) = status.parse()
            {
                emp.status = s;
            }
            if set_doc.contains_key("notes") {
                emp.notes = set_doc.get_str("notes").ok().map(|s| s.to_string());
            }
            if set_doc.contains_key("updated_by_device") {
                emp.updated_by_device = set_doc
                    .get_str("updated_by_device")
                    .ok()
                    .map(|s| s.to_string());
            }
            emp.version += 1;
            emp.updated_at = BsonDateTime::now();

            let updated_at_iso = bson_to_iso(&emp.updated_at);

            sqlx::query(
                r#"
                UPDATE employees SET
                    name = ?, phone = ?, nic_or_id = ?, role = ?, default_split_type = ?,
                    default_split_value = ?, status = ?, notes = ?, updated_by_device = ?,
                    version = ?, updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                "#,
            )
            .bind(&emp.name)
            .bind(&emp.phone)
            .bind(&emp.nic_or_id)
            .bind(emp.role.as_str())
            .bind(emp.default_split_type.as_str())
            .bind(emp.default_split_value)
            .bind(emp.status.as_str())
            .bind(&emp.notes)
            .bind(&emp.updated_by_device)
            .bind(emp.version)
            .bind(&updated_at_iso)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(emp))
        }
    }
}

pub(crate) async fn delete_employee(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<EmployeeDocument>> {
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
            Ok(employees(db)
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
            let existing = find_employee_by_id(db, id).await?;
            let Some(mut emp) = existing else {
                return Ok(None);
            };

            let now = BsonDateTime::now();
            emp.deleted_at = Some(now);
            emp.updated_at = now;
            emp.version += 1;
            emp.updated_by_device = device_id.clone();

            let now_iso = bson_to_iso(&now);

            sqlx::query(
                "UPDATE employees SET deleted_at = ?, updated_at = ?, updated_by_device = ?, version = version + 1 WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&now_iso)
            .bind(&now_iso)
            .bind(&device_id)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(emp))
        }
    }
}

pub(crate) async fn list_employees(db: &Db, filter: Document) -> AppResult<Vec<EmployeeDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut effective_filter = filter;
            if !effective_filter.contains_key("deleted_at") {
                effective_filter.insert("deleted_at", doc! { "$exists": false });
            }
            let mut cursor = employees(db)
                .find(effective_filter)
                .sort(doc! { "name": 1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query_as::<_, EmployeeSqliteRow>(
                "SELECT * FROM employees WHERE deleted_at IS NULL ORDER BY name ASC",
            )
            .fetch_all(pool)
            .await?;

            let mut items: Vec<EmployeeDocument> = rows
                .into_iter()
                .map(EmployeeSqliteRow::into_document)
                .collect();

            // Apply in-memory filtering for search, role, status if clauses exist
            if let Ok(and_arr) = filter.get_array("$and") {
                for clause in and_arr {
                    if let Some(clause_doc) = clause.as_document() {
                        if let Ok(role_str) = clause_doc.get_str("role") {
                            items.retain(|e| e.role.as_str() == role_str);
                        }
                        if let Ok(status_str) = clause_doc.get_str("status") {
                            items.retain(|e| e.status.as_str() == status_str);
                        }
                        if let Ok(or_arr) = clause_doc.get_array("$or") {
                            // Search filter
                            let mut pattern = String::new();
                            for cond in or_arr {
                                if let Some(cond_doc) = cond.as_document() {
                                    for key in ["name", "phone", "nic_or_id"] {
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
                                items.retain(|e| {
                                    e.name.to_lowercase().contains(&pattern)
                                        || e.phone.to_lowercase().contains(&pattern)
                                        || e.nic_or_id
                                            .as_ref()
                                            .map(|nic| nic.to_lowercase().contains(&pattern))
                                            .unwrap_or(false)
                                });
                            }
                        }
                    }
                }
            }

            Ok(items)
        }
    }
}
