use super::customers::customers;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

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
