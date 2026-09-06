// Dual-engine (Mongo / SQLite) access for the `suppliers` collection.
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
    modules::suppliers::model::SupplierDocument,
};

#[derive(sqlx::FromRow)]
struct SupplierSqliteRow {
    id: String,
    key: String,
    name: String,
    contact_person: String,
    primary_phone: String,
    secondary_phone: Option<String>,
    email: Option<String>,
    address: String,
    supplied_categories: String,
    notes: Option<String>,
    version: i64,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    updated_by_device: Option<String>,
}

impl SupplierSqliteRow {
    fn into_document(self) -> SupplierDocument {
        let supplied_categories: Vec<String> =
            serde_json::from_str(&self.supplied_categories).unwrap_or_default();
        SupplierDocument {
            id: ObjectId::parse_str(&self.id).ok(),
            key: self.key,
            name: self.name,
            contact_person: self.contact_person,
            primary_phone: self.primary_phone,
            secondary_phone: self.secondary_phone,
            email: self.email,
            address: self.address,
            supplied_categories,
            notes: self.notes,
            version: self.version,
            created_at: to_bson_datetime(&self.created_at),
            updated_at: to_bson_datetime(&self.updated_at),
            deleted_at: self.deleted_at.as_deref().map(to_bson_datetime),
            updated_by_device: self.updated_by_device,
        }
    }
}

fn suppliers(db: &mongodb::Database) -> Collection<SupplierDocument> {
    db.collection("suppliers")
}

pub(crate) async fn find_supplier_by_id(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<SupplierDocument>> {
    match db {
        Db::Mongo(db) => Ok(suppliers(db)
            .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let row = sqlx::query_as::<_, SupplierSqliteRow>(
                "SELECT * FROM suppliers WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&id_str)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(SupplierSqliteRow::into_document))
        }
    }
}

pub(crate) async fn find_supplier_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<SupplierDocument>> {
    match db {
        Db::Mongo(db) => Ok(suppliers(db)
            .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query_as::<_, SupplierSqliteRow>(
                "SELECT * FROM suppliers WHERE key = ? AND deleted_at IS NULL",
            )
            .bind(key)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(SupplierSqliteRow::into_document))
        }
    }
}

pub(crate) async fn find_suppliers_by_keys(
    db: &Db,
    keys: &[String],
) -> AppResult<Vec<SupplierDocument>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = suppliers(db)
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
                "SELECT * FROM suppliers WHERE key IN ({placeholders}) AND deleted_at IS NULL"
            );
            let mut query = sqlx::query_as::<_, SupplierSqliteRow>(&sql);
            for key in keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            Ok(rows
                .into_iter()
                .map(SupplierSqliteRow::into_document)
                .collect())
        }
    }
}

pub(crate) async fn insert_supplier(
    db: &Db,
    mut document: SupplierDocument,
) -> AppResult<SupplierDocument> {
    match db {
        Db::Mongo(db) => {
            let result = suppliers(db).insert_one(&document).await?;
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
            let categories_json = serde_json::to_string(&document.supplied_categories)
                .unwrap_or_else(|_| "[]".to_string());
            let created_at = bson_to_iso(&document.created_at);
            let updated_at = bson_to_iso(&document.updated_at);
            let deleted_at = document.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO suppliers (
                    key, id, name, contact_person, primary_phone, secondary_phone,
                    email, address, supplied_categories, notes, version,
                    created_at, updated_at, deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            .bind(&categories_json)
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

pub(crate) async fn update_supplier(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<SupplierDocument>> {
    match db {
        Db::Mongo(db) => Ok(suppliers(db)
            .find_one_and_update(
                doc! { "_id": id, "deleted_at": { "$exists": false } },
                doc! { "$set": set_doc, "$inc": { "version": 1 } },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let existing = find_supplier_by_id(db, id).await?;
            let Some(mut sup) = existing else {
                return Ok(None);
            };

            if let Ok(name) = set_doc.get_str("name") {
                sup.name = name.to_string();
            }
            if let Ok(contact) = set_doc.get_str("contact_person") {
                sup.contact_person = contact.to_string();
            }
            if let Ok(phone) = set_doc.get_str("primary_phone") {
                sup.primary_phone = phone.to_string();
            }
            if set_doc.contains_key("secondary_phone") {
                sup.secondary_phone = set_doc
                    .get_str("secondary_phone")
                    .ok()
                    .map(|s| s.to_string());
            }
            if set_doc.contains_key("email") {
                sup.email = set_doc.get_str("email").ok().map(|s| s.to_string());
            }
            if let Ok(addr) = set_doc.get_str("address") {
                sup.address = addr.to_string();
            }
            if let Ok(cats_arr) = set_doc.get_array("supplied_categories") {
                sup.supplied_categories = cats_arr
                    .iter()
                    .filter_map(|b| b.as_str().map(str::to_string))
                    .collect();
            }
            if set_doc.contains_key("notes") {
                sup.notes = set_doc.get_str("notes").ok().map(|s| s.to_string());
            }
            if set_doc.contains_key("updated_by_device") {
                sup.updated_by_device = set_doc
                    .get_str("updated_by_device")
                    .ok()
                    .map(|s| s.to_string());
            }
            sup.version += 1;
            sup.updated_at = BsonDateTime::now();

            let categories_json = serde_json::to_string(&sup.supplied_categories)
                .unwrap_or_else(|_| "[]".to_string());
            let updated_at_iso = bson_to_iso(&sup.updated_at);

            sqlx::query(
                r#"
                UPDATE suppliers SET
                    name = ?, contact_person = ?, primary_phone = ?, secondary_phone = ?,
                    email = ?, address = ?, supplied_categories = ?, notes = ?,
                    updated_by_device = ?, version = ?, updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                "#,
            )
            .bind(&sup.name)
            .bind(&sup.contact_person)
            .bind(&sup.primary_phone)
            .bind(&sup.secondary_phone)
            .bind(&sup.email)
            .bind(&sup.address)
            .bind(&categories_json)
            .bind(&sup.notes)
            .bind(&sup.updated_by_device)
            .bind(sup.version)
            .bind(&updated_at_iso)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(sup))
        }
    }
}

pub(crate) async fn delete_supplier(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<SupplierDocument>> {
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
            Ok(suppliers(db)
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
            let existing = find_supplier_by_id(db, id).await?;
            let Some(mut sup) = existing else {
                return Ok(None);
            };

            let now = BsonDateTime::now();
            sup.deleted_at = Some(now);
            sup.updated_at = now;
            sup.version += 1;
            sup.updated_by_device = device_id.clone();

            let now_iso = bson_to_iso(&now);

            sqlx::query(
                "UPDATE suppliers SET deleted_at = ?, updated_at = ?, updated_by_device = ?, version = version + 1 WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&now_iso)
            .bind(&now_iso)
            .bind(&device_id)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(sup))
        }
    }
}

pub(crate) async fn list_suppliers(db: &Db, filter: Document) -> AppResult<Vec<SupplierDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut effective_filter = filter;
            if !effective_filter.contains_key("deleted_at") {
                effective_filter.insert("deleted_at", doc! { "$exists": false });
            }
            let mut cursor = suppliers(db)
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
            let rows = sqlx::query_as::<_, SupplierSqliteRow>(
                "SELECT * FROM suppliers WHERE deleted_at IS NULL ORDER BY name ASC",
            )
            .fetch_all(pool)
            .await?;

            let mut items: Vec<SupplierDocument> = rows
                .into_iter()
                .map(SupplierSqliteRow::into_document)
                .collect();

            // In-memory filter for category and search if clauses exist
            if let Ok(and_arr) = filter.get_array("$and") {
                for clause in and_arr {
                    if let Some(clause_doc) = clause.as_document() {
                        if let Ok(cat_str) = clause_doc.get_str("supplied_categories") {
                            items.retain(|s| s.supplied_categories.iter().any(|c| c == cat_str));
                        }
                        if let Ok(or_arr) = clause_doc.get_array("$or") {
                            let mut pattern = String::new();
                            for cond in or_arr {
                                if let Some(cond_doc) = cond.as_document() {
                                    for key in ["name", "contact_person", "primary_phone", "email"]
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
                                items.retain(|s| {
                                    s.name.to_lowercase().contains(&pattern)
                                        || s.contact_person.to_lowercase().contains(&pattern)
                                        || s.primary_phone.to_lowercase().contains(&pattern)
                                        || s.email
                                            .as_ref()
                                            .map(|e| e.to_lowercase().contains(&pattern))
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

pub(crate) async fn distinct_supplied_categories(db: &Db) -> AppResult<Vec<String>> {
    match db {
        Db::Mongo(db) => {
            let values = suppliers(db)
                .distinct("supplied_categories", doc! {})
                .await?;

            Ok(values
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect())
        }
        Db::Sqlite(pool) => {
            let rows: Vec<String> = sqlx::query_scalar(
                "SELECT supplied_categories FROM suppliers WHERE deleted_at IS NULL",
            )
            .fetch_all(pool)
            .await?;

            let mut set = std::collections::BTreeSet::new();
            for cats_json in rows {
                if let Ok(cats) = serde_json::from_str::<Vec<String>>(&cats_json) {
                    for cat in cats {
                        set.insert(cat);
                    }
                }
            }
            Ok(set.into_iter().collect())
        }
    }
}

pub(crate) async fn count_stats(db: &Db) -> AppResult<(u64, u64)> {
    match db {
        Db::Mongo(db) => {
            let total_suppliers = suppliers(db)
                .count_documents(doc! { "deleted_at": { "$exists": false } })
                .await?;
            let direct_contacts_count = suppliers(db)
                .count_documents(doc! {
                    "deleted_at": { "$exists": false },
                    "contact_person": { "$ne": "" }
                })
                .await?;
            Ok((total_suppliers, direct_contacts_count))
        }
        Db::Sqlite(pool) => {
            #[derive(sqlx::FromRow)]
            struct SupplierCounts {
                total: i64,
                direct: i64,
            }

            let counts = sqlx::query_as::<_, SupplierCounts>(
                r#"
                SELECT
                    COUNT(*) as total,
                    COUNT(CASE WHEN contact_person IS NOT NULL AND contact_person != '' THEN 1 ELSE NULL END) as direct
                FROM suppliers
                WHERE deleted_at IS NULL
                "#,
            )
            .fetch_one(pool)
            .await?;

            Ok((counts.total as u64, counts.direct as u64))
        }
    }
}
