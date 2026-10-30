use super::suppliers::suppliers;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

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
