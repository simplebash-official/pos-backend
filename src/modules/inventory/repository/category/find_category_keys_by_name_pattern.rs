use super::categories::categories;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use futures_util::TryStreamExt;

use mongodb::bson::doc;

use sqlx::Row;

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
                .map_err(|err| AppError::validation(format!("Invalid name pattern: {err}")))?;

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
