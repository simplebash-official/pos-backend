use super::employee_sqlite_row::EmployeeSqliteRow;
use super::employees::employees;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::employees::model::EmployeeDocument;
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};

pub(crate) async fn list_employees(db: &Db, filter: Document) -> AppResult<Vec<EmployeeDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut effective_filter = filter;
            if !effective_filter.contains_key("deleted_at") {
                effective_filter.insert("deleted_at", doc! { "$exists": false });
            }
            let mut cursor = employees(db)
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
            let rows = sqlx::query_as::<_, EmployeeSqliteRow>(
                "SELECT * FROM employees WHERE deleted_at IS NULL ORDER BY name ASC",
            )
            .fetch_all(pool)
            .await?;

            let mut items: Vec<EmployeeDocument> = rows
                .into_iter()
                .map(EmployeeSqliteRow::into_document)
                .collect();

            // Apply in-memory filtering for search, role, status if clauses exist
            if let Ok(and_arr) = filter.get_array("$and") {
                for clause in and_arr {
                    if let Some(clause_doc) = clause.as_document() {
                        if let Ok(role_str) = clause_doc.get_str("role") {
                            items.retain(|e| e.role.as_str() == role_str);
                        }
                        if let Ok(status_str) = clause_doc.get_str("status") {
                            items.retain(|e| e.status.as_str() == status_str);
                        }
                        if let Ok(or_arr) = clause_doc.get_array("$or") {
                            // Search filter
                            let mut pattern = String::new();
                            for cond in or_arr {
                                if let Some(cond_doc) = cond.as_document() {
                                    for key in ["name", "phone", "nic_or_id"] {
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
                                items.retain(|e| {
                                    e.name.to_lowercase().contains(&pattern)
                                        || e.phone.to_lowercase().contains(&pattern)
                                        || e.nic_or_id
                                            .as_ref()
                                            .map(|nic| nic.to_lowercase().contains(&pattern))
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
