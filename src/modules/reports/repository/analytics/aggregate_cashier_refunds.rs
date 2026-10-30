use super::super::sales::get_i64_flexible;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

/// `cashier_id -> (refund credit-note count, Σ net_refund_cents)` over
/// non-voided credit notes in `[start, end)`.
pub(crate) async fn aggregate_cashier_refunds(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<std::collections::HashMap<String, (u64, i64)>> {
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
                        "_id": "$cashier_id",
                        "count": { "$sum": 1 },
                        "amount": { "$sum": "$net_refund_cents" },
                    }
                },
            ];
            let mut cursor = db
                .collection::<Document>("credit_notes")
                .aggregate(pipeline)
                .await?;
            let mut out = std::collections::HashMap::new();
            while let Some(row) = cursor.try_next().await? {
                if let Ok(cid) = row.get_str("_id") {
                    out.insert(
                        cid.to_string(),
                        (
                            get_i64_flexible(&row, "count") as u64,
                            get_i64_flexible(&row, "amount"),
                        ),
                    );
                }
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            use sqlx::Row;
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT cashier_id, COUNT(*) as cnt, COALESCE(SUM(net_refund_cents), 0) as amount FROM credit_notes WHERE status != 'voided' AND created_at >= ? AND created_at < ? GROUP BY cashier_id"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut out = std::collections::HashMap::new();
            for r in rows {
                let cid: Option<String> = r.get("cashier_id");
                if let Some(cid) = cid {
                    let cnt: i64 = r.get("cnt");
                    let amt: i64 = r.get("amount");
                    out.insert(cid, (cnt as u64, amt));
                }
            }
            Ok(out)
        }
    }
}
