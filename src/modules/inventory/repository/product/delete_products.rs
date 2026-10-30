use super::products::products;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

pub(crate) async fn delete_products(db: &Db, ids: Vec<ObjectId>) -> AppResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    match db {
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            let result = products(db)
                .update_many(
                    doc! { "_id": { "$in": ids }, "deleted_at": { "$exists": false } },
                    doc! {
                        "$set": { "deleted_at": now, "updated_at": now },
                        "$inc": { "version": 1 }
                    },
                )
                .await?;
            Ok(result.modified_count)
        }
        Db::Sqlite(pool) => {
            let now_iso = bson_to_iso(&BsonDateTime::now());
            let placeholders: Vec<_> = ids.iter().map(|_| "?").collect();
            let query_str = format!(
                "UPDATE products SET deleted_at = ?, updated_at = ?, version = version + 1 WHERE id IN ({}) AND deleted_at IS NULL",
                placeholders.join(",")
            );
            let mut q = sqlx::query(&query_str).bind(&now_iso).bind(&now_iso);
            for id in ids {
                q = q.bind(id.to_hex());
            }
            let res = q.execute(pool).await?;
            Ok(res.rows_affected())
        }
    }
}
