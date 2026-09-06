// Mongo access for the `categories` collection only. This collection is
// the single source of truth for both `GET /categories`-style reads and
// (together with `repository::subcategory`) product validation
// (`service::product::ensure_valid_category`) — there is no separate
// hardcoded category list anywhere else in the codebase.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc, oid::ObjectId},
    options::ReturnDocument,
};
use serde::Deserialize;
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
    },
    core::error::AppResult,
    modules::inventory::model::{CategoryDocument, SubcategoryDocument},
};

fn categories(db: &Database) -> Collection<CategoryDocument> {
    db.collection("categories")
}

fn row_to_category_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<CategoryDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(CategoryDocument {
        id: object_id,
        key: row.get("key"),
        name: row.get("name"),
        icon: row.get("icon"),
        color: row.get("color"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
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

/// For `GET /inventory/stats`. Categories are hard-deleted (`delete_category`
/// below), not soft-deleted, so a plain count is the live total.
pub(crate) async fn count_categories(db: &Db) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(categories(db).count_documents(doc! {}).await?),
        Db::Sqlite(pool) => {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories")
                .fetch_one(pool)
                .await?;
            Ok(count as u64)
        }
    }
}

/// Only used for the create-time name-uniqueness check
/// (`service::category::create_category`/`update_category`) — every other
/// lookup goes by `key` (see `find_category_by_key`).
pub(crate) async fn find_category_by_name(
    db: &Db,
    name: &str,
) -> AppResult<Option<CategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(categories(db).find_one(doc! { "name": name }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM categories WHERE name = ?")
                .bind(name)
                .fetch_optional(pool)
                .await?;

            row.map(|r| row_to_category_doc(&r)).transpose()
        }
    }
}

pub(crate) async fn find_category_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<CategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(categories(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM categories WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;

            row.map(|r| row_to_category_doc(&r)).transpose()
        }
    }
}

#[derive(Debug, Deserialize)]
struct CategoryWithSubcategories {
    #[serde(flatten)]
    category: CategoryDocument,
    #[serde(default)]
    subcategories: Vec<SubcategoryDocument>,
}

/// Runs a `$lookup` aggregation pipeline joining `categories` to their
/// `subcategories` (on `key`/`category_key`) in a single round trip.
pub(crate) async fn list_categories_with_subcategories(
    db: &Db,
) -> AppResult<Vec<(CategoryDocument, Vec<SubcategoryDocument>)>> {
    match db {
        Db::Mongo(db) => {
            let pipeline = vec![
                doc! { "$sort": { "name": 1 } },
                doc! {
                    "$lookup": {
                        "from": "subcategories",
                        "localField": "key",
                        "foreignField": "category_key",
                        "as": "subcategories",
                    }
                },
            ];

            let mut cursor = categories(db).aggregate(pipeline).await?;
            let mut items = Vec::new();
            while let Some(doc) = cursor.try_next().await? {
                let mut item = bson::deserialize_from_document::<CategoryWithSubcategories>(doc)?;
                item.subcategories.sort_by(|a, b| a.name.cmp(&b.name));
                items.push((item.category, item.subcategories));
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let cat_rows = sqlx::query("SELECT * FROM categories ORDER BY name ASC")
                .fetch_all(pool)
                .await?;
            let sub_rows = sqlx::query("SELECT * FROM subcategories ORDER BY name ASC")
                .fetch_all(pool)
                .await?;

            let mut sub_by_cat: std::collections::HashMap<String, Vec<SubcategoryDocument>> =
                std::collections::HashMap::new();

            for r in &sub_rows {
                let sub = row_to_subcategory_doc(r)?;
                sub_by_cat
                    .entry(sub.category_key.clone())
                    .or_default()
                    .push(sub);
            }

            let mut items = Vec::new();
            for r in &cat_rows {
                let cat = row_to_category_doc(r)?;
                let mut subs = sub_by_cat.remove(&cat.key).unwrap_or_default();
                subs.sort_by(|a, b| a.name.cmp(&b.name));
                items.push((cat, subs));
            }

            Ok(items)
        }
    }
}

pub(crate) async fn insert_category(db: &Db, document: &CategoryDocument) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            categories(db).insert_one(document).await?;
            Ok(())
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
                INSERT INTO categories (
                    key, id, name, icon, color, version, created_at, updated_at,
                    deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.name)
            .bind(&document.icon)
            .bind(&document.color)
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

/// `set_doc` is assembled by the caller (`service::category::update_category`);
/// this just applies it, filtered by the category's immutable `key` rather
/// than its (renameable) `name`.
pub(crate) async fn update_category_fields(
    db: &Db,
    key: &str,
    set_doc: Document,
) -> AppResult<Option<CategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(categories(db)
            .find_one_and_update(doc! { "key": key }, doc! { "$set": set_doc })
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let existing = find_category_by_key(db, key).await?;
            let Some(mut cat) = existing else {
                return Ok(None);
            };

            if let Ok(name) = set_doc.get_str("name") {
                cat.name = name.to_string();
            }
            if let Ok(icon) = set_doc.get_str("icon") {
                cat.icon = icon.to_string();
            }
            if let Ok(color) = set_doc.get_str("color") {
                cat.color = color.to_string();
            }
            if let Ok(dt) = set_doc.get_datetime("updated_at") {
                cat.updated_at = *dt;
            }
            cat.version += 1;

            let updated_at_iso = bson_to_iso(&cat.updated_at);

            sqlx::query(
                "UPDATE categories SET name = ?, icon = ?, color = ?, version = ?, updated_at = ? WHERE key = ?",
            )
            .bind(&cat.name)
            .bind(&cat.icon)
            .bind(&cat.color)
            .bind(cat.version)
            .bind(&updated_at_iso)
            .bind(key)
            .execute(pool)
            .await?;

            Ok(Some(cat))
        }
    }
}

pub(crate) async fn delete_category(db: &Db, key: &str) -> AppResult<Option<CategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(categories(db)
            .find_one_and_delete(doc! { "key": key })
            .await?),
        Db::Sqlite(pool) => {
            let existing = find_category_by_key(db, key).await?;
            if existing.is_some() {
                sqlx::query("DELETE FROM categories WHERE key = ?")
                    .bind(key)
                    .execute(pool)
                    .await?;
            }
            Ok(existing)
        }
    }
}

pub(crate) async fn find_category_keys_by_name_pattern(
    db: &Db,
    pattern: &mongodb::bson::Regex,
) -> AppResult<Vec<String>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = categories(db)
                .find(doc! { "name": { "$regex": pattern } })
                .await?;
            let mut keys = Vec::new();
            while let Some(doc) = cursor.try_next().await? {
                keys.push(doc.key);
            }
            Ok(keys)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT key, name FROM categories")
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
