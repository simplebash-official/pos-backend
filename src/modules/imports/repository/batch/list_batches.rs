use super::import_batches::import_batches;
use super::map_sqlite_row_to_batch::map_sqlite_row_to_batch;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::imports::model::ImportBatchDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

pub(crate) async fn list_batches(
    db: &Db,
    target: Option<&str>,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<ImportBatchDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let mut filter = doc! {};
            if let Some(t) = target.filter(|s| !s.is_empty()) {
                filter.insert("target", t);
            }

            let total = import_batches(db).count_documents(filter.clone()).await?;

            let mut cursor = import_batches(db)
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .skip(skip)
                .limit(limit as i64)
                .await?;

            let mut items = Vec::new();
            while let Some(doc) = cursor.try_next().await? {
                items.push(doc);
            }

            Ok((items, total))
        }
        Db::Sqlite(pool) => {
            let (where_clause, bind_target) = match target.filter(|s| !s.is_empty()) {
                Some(t) => ("WHERE target = ?", Some(t)),
                None => ("", None),
            };

            let count_query_str = format!("SELECT COUNT(*) FROM import_batches {where_clause}");
            let mut count_query = sqlx::query_scalar::<_, i64>(&count_query_str);
            if let Some(t) = bind_target {
                count_query = count_query.bind(t);
            }
            let total: i64 = count_query.fetch_one(pool).await?;

            let list_query_str = format!(
                "SELECT * FROM import_batches {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut list_query = sqlx::query(&list_query_str);
            if let Some(t) = bind_target {
                list_query = list_query.bind(t);
            }
            let rows = list_query
                .bind(limit as i64)
                .bind(skip as i64)
                .fetch_all(pool)
                .await?;

            let mut items = Vec::new();
            for row in rows {
                items.push(map_sqlite_row_to_batch(&row)?);
            }
            Ok((items, total as u64))
        }
    }
}
