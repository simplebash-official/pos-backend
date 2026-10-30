use super::supplier_sqlite_row::SupplierSqliteRow;
use super::suppliers::suppliers;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::suppliers::model::SupplierDocument;
use futures_util::TryStreamExt;

use mongodb::bson::{Document, doc};

pub(crate) async fn list_suppliers(db: &Db, filter: Document) -> AppResult<Vec<SupplierDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut effective_filter = filter;
            if !effective_filter.contains_key("deleted_at") {
                effective_filter.insert("deleted_at", doc! { "$exists": false });
            }
            let mut cursor = suppliers(db)
                .find(effective_filter)
                .sort(doc! { "name": 1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query_as::<_, SupplierSqliteRow>(
                "SELECT * FROM suppliers WHERE deleted_at IS NULL ORDER BY name ASC",
            )
            .fetch_all(pool)
            .await?;

            let mut items: Vec<SupplierDocument> = rows
                .into_iter()
                .map(SupplierSqliteRow::into_document)
                .collect();

            // In-memory filter for category and search if clauses exist
            if let Ok(and_arr) = filter.get_array("$and") {
                for clause in and_arr {
                    if let Some(clause_doc) = clause.as_document() {
                        if let Ok(cat_str) = clause_doc.get_str("supplied_categories") {
                            items.retain(|s| s.supplied_categories.iter().any(|c| c == cat_str));
                        }
                        if let Ok(or_arr) = clause_doc.get_array("$or") {
                            let mut pattern = String::new();
                            for cond in or_arr {
                                if let Some(cond_doc) = cond.as_document() {
                                    for key in ["name", "contact_person", "primary_phone", "email"]
                                    {
                                        if let Ok(field_doc) = cond_doc.get_document(key)
                                            && let Ok(regex) = field_doc.get_str("$regex")
                                        {
                                            pattern = regex.to_lowercase();
                                            break;
                                        }
                                    }
                                }
                                if !pattern.is_empty() {
                                    break;
                                }
                            }
                            if !pattern.is_empty() {
                                items.retain(|s| {
                                    s.name.to_lowercase().contains(&pattern)
                                        || s.contact_person.to_lowercase().contains(&pattern)
                                        || s.primary_phone.to_lowercase().contains(&pattern)
                                        || s.contact_persons.iter().any(|cp| {
                                            cp.name.to_lowercase().contains(&pattern)
                                                || cp
                                                    .phone_numbers
                                                    .iter()
                                                    .any(|p| p.to_lowercase().contains(&pattern))
                                        })
                                        || s.email
                                            .as_ref()
                                            .map(|e| e.to_lowercase().contains(&pattern))
                                            .unwrap_or(false)
                                });
                            }
                        }
                    }
                }
            }

            Ok(items)
        }
    }
}
