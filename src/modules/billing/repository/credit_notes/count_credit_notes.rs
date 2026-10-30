use super::build_credit_note_where_clause::build_credit_note_where_clause;
use super::credit_notes_collection::credit_notes_collection;
use crate::clients::db::Db;
use crate::clients::sqlite::SqlFilter;

use crate::core::error::AppResult;

use mongodb::bson::Document;
use sqlx::Row;

/// Returns the count of credit note documents matching the specified filter.
pub(crate) async fn count_credit_notes(db: &Db, filter: Document) -> AppResult<u64> {
    match db {
        Db::Mongo(db) => Ok(credit_notes_collection(db).count_documents(filter).await?),
        Db::Sqlite(pool) => {
            let SqlFilter {
                clause: where_clause,
                bindings,
            } = build_credit_note_where_clause(&filter);
            let count_sql =
                format!("SELECT COUNT(*) as total FROM credit_notes WHERE {where_clause}");
            let mut count_query = sqlx::query(&count_sql);
            for b in &bindings {
                count_query = count_query.bind(b);
            }
            let count_row = count_query.fetch_one(pool).await?;
            let total: i64 = count_row.get("total");
            Ok(total as u64)
        }
    }
}
