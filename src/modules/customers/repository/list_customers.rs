use super::customer_sqlite_row::CustomerSqliteRow;
use super::customers::customers;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::customers::model::CustomerDocument;
use futures_util::TryStreamExt;

use mongodb::bson::{Document, doc};

pub(crate) async fn list_customers(
    db: &Db,
    filter: Document,
    sort: Document,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<CustomerDocument>, u64)> {
    match db {
        Db::Mongo(db) => {
            let mut effective_filter = filter;
            if !effective_filter.contains_key("deleted_at") {
                effective_filter.insert("deleted_at", doc! { "$exists": false });
            }

            let total = customers(db)
                .count_documents(effective_filter.clone())
                .await?;

            let mut cursor = customers(db)
                .find(effective_filter)
                .sort(sort)
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
            let rows = sqlx::query_as::<_, CustomerSqliteRow>(
                "SELECT * FROM customers WHERE deleted_at IS NULL",
            )
            .fetch_all(pool)
            .await?;

            let mut items: Vec<CustomerDocument> = rows
                .into_iter()
                .map(CustomerSqliteRow::into_document)
                .collect();

            // Filter in-memory by clauses
            if let Ok(and_arr) = filter.get_array("$and") {
                for clause in and_arr {
                    if let Some(clause_doc) = clause.as_document() {
                        if let Ok(tag_str) = clause_doc.get_str("tags") {
                            items.retain(|c| c.tags.iter().any(|t| t == tag_str));
                        }
                        if let Ok(bal_doc) = clause_doc.get_document("outstanding_balance_cents")
                            && let Ok(gt_val) = bal_doc.get_i64("$gt")
                        {
                            items.retain(|c| c.outstanding_balance_cents > gt_val);
                        }
                        if let Ok(or_arr) = clause_doc.get_array("$or") {
                            let mut pattern = String::new();
                            for cond in or_arr {
                                if let Some(cond_doc) = cond.as_document() {
                                    for key in ["name", "primary_phone", "secondary_phone", "email"]
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
                                items.retain(|c| {
                                    c.name.to_lowercase().contains(&pattern)
                                        || c.primary_phone.to_lowercase().contains(&pattern)
                                        || c.secondary_phone
                                            .as_ref()
                                            .map(|p| p.to_lowercase().contains(&pattern))
                                            .unwrap_or(false)
                                        || c.email
                                            .as_ref()
                                            .map(|e| e.to_lowercase().contains(&pattern))
                                            .unwrap_or(false)
                                });
                            }
                        }
                    }
                }
            }

            // Sort
            if let Ok(dir) = sort.get_i32("name") {
                if dir == -1 {
                    items.sort_by(|a, b| b.name.cmp(&a.name));
                } else {
                    items.sort_by(|a, b| a.name.cmp(&b.name));
                }
            } else if let Ok(dir) = sort.get_i32("created_at") {
                if dir == -1 {
                    items.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                } else {
                    items.sort_by_key(|a| a.created_at);
                }
            } else if let Ok(dir) = sort.get_i32("outstanding_balance_cents") {
                if dir == -1 {
                    items.sort_by(|a, b| {
                        b.outstanding_balance_cents
                            .cmp(&a.outstanding_balance_cents)
                    });
                } else {
                    items.sort_by(|a, b| {
                        a.outstanding_balance_cents
                            .cmp(&b.outstanding_balance_cents)
                    });
                }
            }

            let total = items.len() as u64;
            let paged: Vec<CustomerDocument> = items
                .into_iter()
                .skip(skip as usize)
                .take(limit as usize)
                .collect();

            Ok((paged, total))
        }
    }
}
