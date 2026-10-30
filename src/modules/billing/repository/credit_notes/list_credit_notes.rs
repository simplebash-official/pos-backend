use super::build_credit_note_where_clause::build_credit_note_where_clause;
use super::credit_notes_collection::credit_notes_collection;
use crate::clients::db::Db;
use crate::clients::sqlite::SqlFilter;

use crate::core::error::AppResult;
use crate::modules::billing::model::CreditNoteDocument;
use crate::modules::billing::repository::credit_note_from_sqlite_row;
use futures_util::TryStreamExt;

use mongodb::bson::{Document, doc};
use sqlx::Row;

/// Lists credit note documents matching a filter, sorted newest first with pagination.
pub(crate) async fn list_credit_notes(
    db: &Db,
    filter: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<CreditNoteDocument>, u64)> {
    match db {
        Db::Mongo(mongo_db) => {
            let total = credit_notes_collection(mongo_db)
                .count_documents(filter.clone())
                .await?;

            let mut cursor = credit_notes_collection(mongo_db)
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .skip(skip)
                .limit(limit as i64)
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok((items, total))
        }
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

            let select_sql = format!(
                "SELECT * FROM credit_notes WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut select_query = sqlx::query(&select_sql);
            for b in &bindings {
                select_query = select_query.bind(b);
            }
            select_query = select_query.bind(limit as i64).bind(skip as i64);

            let rows = select_query.fetch_all(pool).await?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(credit_note_from_sqlite_row(&row)?);
            }
            Ok((items, total as u64))
        }
    }
}
