use super::refund_totals::RefundTotals;
use crate::clients::db::Db;
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::error::AppResult;
use crate::modules::reports::repository::sales::get_i64_flexible;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use sqlx::Row;

fn credit_notes(db: &TenantDatabase) -> ScopedCollection<Document> {
    db.collection("credit_notes")
}

pub(crate) async fn aggregate_refund_totals(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<RefundTotals> {
    match db {
        Db::Mongo(db) => {
            let pipeline = vec![
                doc! {
                    "$match": {
                        "status": { "$ne": "voided" },
                        "created_at": {
                            "$gte": BsonDateTime::from_chrono(start),
                            "$lt": BsonDateTime::from_chrono(end),
                        }
                    }
                },
                doc! {
                    "$group": {
                        "_id": null,
                        "net_refund_cents": { "$sum": "$net_refund_cents" },
                        "refund_cash_cents": { "$sum": "$refund_cash_cents" },
                        "balance_reduction_cents": { "$sum": "$balance_reduction_cents" },
                        "count": { "$sum": 1 },
                    }
                },
            ];

            let mut cursor = credit_notes(db).aggregate(pipeline).await?;
            if let Some(doc) = cursor.try_next().await? {
                Ok(RefundTotals {
                    net_refund_cents: get_i64_flexible(&doc, "net_refund_cents"),
                    refund_cash_cents: get_i64_flexible(&doc, "refund_cash_cents"),
                    balance_reduction_cents: get_i64_flexible(&doc, "balance_reduction_cents"),
                    credit_note_count: get_i64_flexible(&doc, "count") as u64,
                })
            } else {
                Ok(RefundTotals::default())
            }
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let row = sqlx::query(
                "SELECT 
                    COALESCE(SUM(net_refund_cents), 0) as net_refund,
                    COALESCE(SUM(refund_cash_cents), 0) as refund_cash,
                    COALESCE(SUM(balance_reduction_cents), 0) as balance_red,
                    COUNT(*) as count
                 FROM credit_notes 
                 WHERE status != 'voided' AND created_at >= ? AND created_at < ?",
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_one(pool)
            .await?;

            let count: i64 = row.get("count");
            Ok(RefundTotals {
                net_refund_cents: row.get("net_refund"),
                refund_cash_cents: row.get("refund_cash"),
                balance_reduction_cents: row.get("balance_red"),
                credit_note_count: count as u64,
            })
        }
    }
}
