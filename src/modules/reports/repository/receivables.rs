// MongoDB aggregation queries for credit receivables and overdue balance tracking.

use chrono::{NaiveDate, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
};

use sqlx::Row;

use crate::{
    clients::{db::Db, sqlite::parse_iso_datetime},
    core::{error::AppResult, utils::build_bson_regex},
    domain::reports::OutstandingInvoiceEntry,
};

fn invoices(db: &Database) -> Collection<Document> {
    db.collection("invoices")
}

fn payments(db: &Database) -> Collection<Document> {
    db.collection("payments")
}

#[derive(Debug, Clone, Default)]
pub(crate) struct OutstandingStatsAggregate {
    pub total_outstanding_cents: i64,
    pub total_credit_invoices_count: u64,
    pub overdue_invoices_count: u64,
    pub overdue_amount_cents: i64,
}

/// Computes all-time outstanding credit and overdue aging metrics.
pub(crate) async fn aggregate_outstanding_stats(db: &Db) -> AppResult<OutstandingStatsAggregate> {
    let today_str = Utc::now().date_naive().format("%Y-%m-%d").to_string();
    let mut agg = OutstandingStatsAggregate::default();

    match db {
        Db::Mongo(db) => {
            let pipeline = vec![
                doc! {
                    "$match": {
                        "status": { "$in": ["pending", "partially_paid"] }
                    }
                },
                doc! {
                    "$lookup": {
                        "from": "payments",
                        "localField": "key",
                        "foreignField": "invoice_key",
                        "as": "payments_docs"
                    }
                },
                doc! {
                    "$project": {
                        "total_cents": 1,
                        "due_date": 1,
                        "paid_cents": { "$sum": "$payments_docs.amount_cents" },
                    }
                },
                doc! {
                    "$project": {
                        "total_cents": 1,
                        "due_date": 1,
                        "balance_due": {
                            "$max": [
                                0,
                                { "$subtract": ["$total_cents", "$paid_cents"] }
                            ]
                        }
                    }
                },
                doc! {
                    "$facet": {
                        "totals": [
                            {
                                "$group": {
                                    "_id": null,
                                    "count": { "$sum": 1 },
                                    "total_balance": { "$sum": "$balance_due" },
                                }
                            }
                        ],
                        "overdue": [
                            {
                                "$match": {
                                    "due_date": { "$ne": null, "$lt": &today_str },
                                    "balance_due": { "$gt": 0 }
                                }
                            },
                            {
                                "$group": {
                                    "_id": null,
                                    "count": { "$sum": 1 },
                                    "overdue_balance": { "$sum": "$balance_due" },
                                }
                            }
                        ]
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;

            if let Some(result) = cursor.try_next().await? {
                if let Ok(totals) = result.get_array("totals")
                    && let Some(first) = totals.first().and_then(|v| v.as_document())
                {
                    agg.total_credit_invoices_count =
                        first.get_i32("count").map(|c| c as u64).unwrap_or(0);
                    agg.total_outstanding_cents = first.get_i64("total_balance").unwrap_or(0);
                }

                if let Ok(overdue) = result.get_array("overdue")
                    && let Some(first) = overdue.first().and_then(|v| v.as_document())
                {
                    agg.overdue_invoices_count =
                        first.get_i32("count").map(|c| c as u64).unwrap_or(0);
                    agg.overdue_amount_cents = first.get_i64("overdue_balance").unwrap_or(0);
                }
            }
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                r#"SELECT
                    COALESCE(SUM(i.total_cents - COALESCE(p.paid_cents, 0)), 0) as total_balance,
                    COUNT(*) as count,
                    COALESCE(SUM(CASE WHEN i.due_date IS NOT NULL AND i.due_date < ? AND (i.total_cents - COALESCE(p.paid_cents, 0)) > 0 THEN 1 ELSE 0 END), 0) as overdue_count,
                    COALESCE(SUM(CASE WHEN i.due_date IS NOT NULL AND i.due_date < ? AND (i.total_cents - COALESCE(p.paid_cents, 0)) > 0 THEN (i.total_cents - COALESCE(p.paid_cents, 0)) ELSE 0 END), 0) as overdue_balance
                FROM invoices i
                LEFT JOIN (
                    SELECT invoice_key, SUM(amount_cents) as paid_cents FROM payments GROUP BY invoice_key
                ) p ON i.key = p.invoice_key
                WHERE i.status IN ('pending', 'partially_paid')"#,
            )
            .bind(&today_str)
            .bind(&today_str)
            .fetch_one(pool)
            .await?;

            let count: i64 = row.get("count");
            let overdue_count: i64 = row.get("overdue_count");
            agg.total_credit_invoices_count = count as u64;
            agg.total_outstanding_cents = row.get("total_balance");
            agg.overdue_invoices_count = overdue_count as u64;
            agg.overdue_amount_cents = row.get("overdue_balance");
        }
    }

    Ok(agg)
}

/// Lists paginated outstanding invoices with search.
pub(crate) async fn list_outstanding_invoices(
    db: &Db,
    search: Option<&str>,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<OutstandingInvoiceEntry>, u64)> {
    match db {
        Db::Mongo(db) => {
            let mut match_filter = doc! {
                "status": { "$in": ["pending", "partially_paid"] }
            };

            if let Some(s) = search.filter(|s| !s.trim().is_empty()) {
                let pattern = build_bson_regex(s.trim());
                match_filter.insert(
                    "$or",
                    vec![
                        doc! { "invoice_number": { "$regex": pattern.clone() } },
                        doc! { "customer_name_snapshot": { "$regex": pattern.clone() } },
                        doc! { "customer_phone_snapshot": { "$regex": pattern } },
                    ],
                );
            }

            let total = invoices(db).count_documents(match_filter.clone()).await?;

            let mut cursor = invoices(db)
                .find(match_filter)
                .sort(doc! { "created_at": -1 })
                .skip(skip)
                .limit(limit as i64)
                .await?;

            let today = Utc::now().date_naive();
            let mut raw_docs = Vec::new();
            let mut keys = Vec::new();

            while let Some(doc) = cursor.try_next().await? {
                if let Ok(k) = doc.get_str("key") {
                    keys.push(k.to_string());
                }
                raw_docs.push(doc);
            }

            // Fetch payments for these invoices
            let mut payments_map: std::collections::HashMap<String, i64> =
                std::collections::HashMap::new();
            if !keys.is_empty() {
                let p_cursor = payments(db)
                    .find(doc! { "invoice_key": { "$in": &keys } })
                    .await?;
                let mut p_stream = p_cursor;
                while let Some(pdoc) = p_stream.try_next().await? {
                    if let Ok(ik) = pdoc.get_str("invoice_key")
                        && let Ok(amt) = pdoc.get_i64("amount_cents")
                    {
                        *payments_map.entry(ik.to_string()).or_insert(0) += amt;
                    }
                }
            }

            let mut entries = Vec::new();
            for doc in raw_docs {
                let id = doc
                    .get_object_id("_id")
                    .map(|o: ObjectId| o.to_hex())
                    .unwrap_or_default();
                let key = doc.get_str("key").unwrap_or_default().to_string();
                let invoice_number = doc
                    .get_str("invoice_number")
                    .unwrap_or_default()
                    .to_string();
                let customer_key = doc.get_str("customer_key").ok().map(String::from);
                let customer_name = doc.get_str("customer_name_snapshot").ok().map(String::from);
                let customer_phone = doc
                    .get_str("customer_phone_snapshot")
                    .ok()
                    .map(String::from);
                let total_cents = doc.get_i64("total_cents").unwrap_or(0);
                let amount_paid_cents = payments_map.get(&key).copied().unwrap_or(0);
                let balance_due_cents = (total_cents - amount_paid_cents).max(0);
                let due_date = doc.get_str("due_date").ok().map(String::from);

                let is_overdue = if let Some(ref d) = due_date {
                    NaiveDate::parse_from_str(d, "%Y-%m-%d")
                        .map(|due| due < today)
                        .unwrap_or(false)
                } else {
                    false
                };

                let created_at = doc
                    .get_datetime("created_at")
                    .map(|dt: &BsonDateTime| dt.to_chrono())
                    .unwrap_or_else(|_| Utc::now());

                entries.push(OutstandingInvoiceEntry {
                    id,
                    key,
                    invoice_number,
                    customer_key,
                    customer_name,
                    customer_phone,
                    total_cents,
                    amount_paid_cents,
                    balance_due_cents,
                    due_date,
                    is_overdue,
                    created_at,
                });
            }

            Ok((entries, total))
        }
        Db::Sqlite(pool) => {
            let today = Utc::now().date_naive();
            let (where_search, search_pattern) = match search.filter(|s| !s.trim().is_empty()) {
                Some(s) => (
                    " AND (i.invoice_number LIKE ? OR i.customer_key LIKE ?)",
                    Some(format!("%{}%", s.trim())),
                ),
                None => ("", None),
            };

            let count_sql = format!(
                "SELECT COUNT(*) FROM invoices i WHERE i.status IN ('pending', 'partially_paid'){where_search}"
            );
            let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
            if let Some(ref pat) = search_pattern {
                count_query = count_query.bind(pat).bind(pat);
            }
            let total: i64 = count_query.fetch_one(pool).await?;

            let list_sql = format!(
                r#"SELECT 
                    i.id, i.key, i.invoice_number, i.customer_key,
                    i.total_cents, i.due_date, i.created_at,
                    COALESCE(p.paid_cents, 0) as paid_cents
                FROM invoices i
                LEFT JOIN (
                    SELECT invoice_key, SUM(amount_cents) as paid_cents FROM payments GROUP BY invoice_key
                ) p ON i.key = p.invoice_key
                WHERE i.status IN ('pending', 'partially_paid'){where_search}
                ORDER BY i.created_at DESC
                LIMIT ? OFFSET ?"#
            );
            let mut list_query = sqlx::query(&list_sql);
            if let Some(ref pat) = search_pattern {
                list_query = list_query.bind(pat).bind(pat);
            }
            let rows = list_query
                .bind(limit as i64)
                .bind(skip as i64)
                .fetch_all(pool)
                .await?;

            let mut entries = Vec::new();
            for r in rows {
                let id: String = r.get("id");
                let key: String = r.get("key");
                let invoice_number: String = r.get("invoice_number");
                let customer_key: Option<String> = r.get("customer_key");
                let total_cents: i64 = r.get("total_cents");
                let amount_paid_cents: i64 = r.get("paid_cents");
                let balance_due_cents = (total_cents - amount_paid_cents).max(0);
                let due_date: Option<String> = r.get("due_date");
                let created_at_str: String = r.get("created_at");
                let created_at = parse_iso_datetime(&created_at_str);

                let is_overdue = if let Some(ref d) = due_date {
                    NaiveDate::parse_from_str(d, "%Y-%m-%d")
                        .map(|due| due < today)
                        .unwrap_or(false)
                } else {
                    false
                };

                entries.push(OutstandingInvoiceEntry {
                    id,
                    key,
                    invoice_number,
                    customer_key,
                    customer_name: None,
                    customer_phone: None,
                    total_cents,
                    amount_paid_cents,
                    balance_due_cents,
                    due_date,
                    is_overdue,
                    created_at,
                });
            }

            Ok((entries, total as u64))
        }
    }
}
