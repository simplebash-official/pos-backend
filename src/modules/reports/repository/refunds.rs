// MongoDB aggregation queries for credit notes and return refunds.

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc},
};

use crate::core::error::AppResult;

fn credit_notes(db: &Database) -> Collection<Document> {
    db.collection("credit_notes")
}

/// Computes the sum of refund amounts issued via completed credit notes in `[start, end)`.
pub(crate) async fn aggregate_refunds_total(
    db: &Database,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<i64> {
    let pipeline = vec![
        doc! {
            "$match": {
                "status": "completed",
                "created_at": {
                    "$gte": BsonDateTime::from_chrono(start),
                    "$lt": BsonDateTime::from_chrono(end),
                }
            }
        },
        doc! {
            "$group": {
                "_id": null,
                "total_refunds": { "$sum": "$refund_amount_cents" }
            }
        },
    ];

    let mut cursor = credit_notes(db).aggregate(pipeline).await?;
    if let Some(doc) = cursor.try_next().await? {
        Ok(doc.get_i64("total_refunds").unwrap_or(0))
    } else {
        Ok(0)
    }
}
