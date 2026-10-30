use super::categories::categories;
use super::row_to_category_doc::row_to_category_doc;
use crate::clients::db::Db;
use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::modules::inventory::model::{CategoryDocument, SubcategoryDocument};
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

use serde::Deserialize;
use sqlx::Row;

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

#[derive(Debug, Deserialize)]
pub(super) struct CategoryWithSubcategories {
    #[serde(flatten)]
    pub(super) category: CategoryDocument,
    #[serde(default)]
    pub(super) subcategories: Vec<SubcategoryDocument>,
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
