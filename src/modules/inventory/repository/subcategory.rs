// Mongo access for the `subcategories` collection only. Every subcategory
// belongs to exactly one category (`category_key`), but this collection has
// no opinion on whether that category still exists — that's a `service`
// concern (see `service::category`).

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
    },
    core::error::AppResult,
    modules::inventory::model::SubcategoryDocument,
};

fn subcategories(db: &TenantDatabase) -> ScopedCollection<SubcategoryDocument> {
    db.collection("subcategories")
}

fn row_to_subcategory_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<SubcategoryDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(SubcategoryDocument {
        id: object_id,
        key: row.get("key"),
        category_key: row.get("category_key"),
        name: row.get("name"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// For `GET /inventory/stats`. Subcategories are hard-deleted, not
/// soft-deleted, so a plain count is the live total.
pub(crate) async fn count_subcategories(db: &Db) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(subcategories(db).count_documents(doc! {}).await?),
        Db::Sqlite(pool) => {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subcategories")
                .fetch_one(pool)
                .await?;
            Ok(count as u64)
        }
    }
}

pub(crate) async fn find_subcategory_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<SubcategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(subcategories(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM subcategories WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;

            row.map(|r| row_to_subcategory_doc(&r)).transpose()
        }
    }
}

/// Backs the duplicate-name guard on `service::category::add_subcategory` —
/// names only need to be unique within a single category, not globally.
pub(crate) async fn find_subcategory_by_category_and_name(
    db: &Db,
    category_key: &str,
    name: &str,
) -> AppResult<Option<SubcategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(subcategories(db)
            .find_one(doc! { "category_key": category_key, "name": name })
            .await?),
        Db::Sqlite(pool) => {
            let row =
                sqlx::query("SELECT * FROM subcategories WHERE category_key = ? AND name = ?")
                    .bind(category_key)
                    .bind(name)
                    .fetch_optional(pool)
                    .await?;

            row.map(|r| row_to_subcategory_doc(&r)).transpose()
        }
    }
}

/// Sorted by name for stable, predictable output.
pub(crate) async fn list_subcategories_by_category(
    db: &Db,
    category_key: &str,
) -> AppResult<Vec<SubcategoryDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = subcategories(db)
                .find(doc! { "category_key": category_key })
                .sort(doc! { "name": 1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows =
                sqlx::query("SELECT * FROM subcategories WHERE category_key = ? ORDER BY name ASC")
                    .bind(category_key)
                    .fetch_all(pool)
                    .await?;

            rows.iter().map(row_to_subcategory_doc).collect()
        }
    }
}

/// Marks the parent category changed. A subcategory travels folded into its
/// category, so devices only learn about a subcategory change through a newer
/// version of the category (the device-side capture triggers do the same).
async fn touch_parent_category(db: &TenantDatabase, category_key: &str) -> AppResult<()> {
    db.collection::<Document>("categories")
        .update_one(
            doc! { "key": category_key },
            doc! { "$set": { "updated_at": BsonDateTime::now() }, "$inc": { "version": 1 } },
        )
        .await?;
    Ok(())
}

pub(crate) async fn insert_subcategory(db: &Db, document: &SubcategoryDocument) -> AppResult<()> {
    match db {
        Db::Mongo(mongo) => {
            subcategories(mongo).insert_one(document).await?;
            touch_parent_category(mongo, &document.category_key).await
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
                INSERT INTO subcategories (
                    key, id, category_key, name, version, created_at, updated_at,
                    deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.category_key)
            .bind(&document.name)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .bind(&deleted_at_iso)
            .bind(&document.updated_by_device)
            .execute(pool)
            .await?;

            Ok(())
        }
    }
}

pub(crate) async fn delete_subcategory_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<SubcategoryDocument>> {
    match db {
        Db::Mongo(mongo) => {
            let deleted = subcategories(mongo)
                .find_one_and_delete(doc! { "key": key })
                .await?;
            if let Some(sub) = &deleted {
                touch_parent_category(mongo, &sub.category_key).await?;
            }
            Ok(deleted)
        }
        Db::Sqlite(pool) => {
            let existing = find_subcategory_by_key(db, key).await?;
            if existing.is_some() {
                sqlx::query("DELETE FROM subcategories WHERE key = ?")
                    .bind(key)
                    .execute(pool)
                    .await?;
            }
            Ok(existing)
        }
    }
}

pub(crate) async fn delete_subcategories_by_category(
    db: &Db,
    category_key: &str,
) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(subcategories(db)
            .delete_many(doc! { "category_key": category_key })
            .await?
            .deleted_count),
        Db::Sqlite(pool) => {
            let res = sqlx::query("DELETE FROM subcategories WHERE category_key = ?")
                .bind(category_key)
                .execute(pool)
                .await?;
            Ok(res.rows_affected())
        }
    }
}

pub(crate) async fn find_subcategory_keys_by_name_pattern(
    db: &Db,
    pattern: &mongodb::bson::Regex,
) -> AppResult<Vec<String>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = subcategories(db)
                .find(doc! { "name": { "$regex": pattern } })
                .await?;
            let mut keys = Vec::new();
            while let Some(doc) = cursor.try_next().await? {
                keys.push(doc.key);
            }
            Ok(keys)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT key, name FROM subcategories")
                .fetch_all(pool)
                .await?;

            let pat_str = pattern.pattern.as_str();
            let opt_str = pattern.options.as_str();
            let re = regex::RegexBuilder::new(pat_str)
                .case_insensitive(opt_str.contains('i'))
                .build()
                .unwrap_or_else(|_| regex::Regex::new(".*").unwrap());

            let mut keys = Vec::new();
            for r in rows {
                let name: String = r.get("name");
                if re.is_match(&name) {
                    keys.push(r.get("key"));
                }
            }
            Ok(keys)
        }
    }
}
