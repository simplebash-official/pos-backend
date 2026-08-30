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

use crate::modules::reports::repository::sales::get_i64_flexible;

/// Aggregate refund totals over non-voided credit notes in `[start, end)`.
///
/// The pre-fix version of this query matched `status: "completed"` and summed
/// `refund_amount_cents` — neither exists on a `CreditNoteDocument`. Its
/// `status` enum is `resolved` / `awaiting_resolution` / `voided`
/// (`domain::billing::CreditNoteStatus`) and the money fields are
/// `net_refund_cents` (value of the return before the partial-payment cap),
/// `refund_cash_cents` (paid out in cash), and `balance_reduction_cents`
/// (applied against an invoice balance instead). So `total_refunds` was
/// silently always 0. All three real figures are surfaced here; callers pick
/// the one that fits their P&L definition.
// `refund_cash_cents` / `balance_reduction_cents` / `credit_note_count` are
// consumed by the dedicated `GET /reports/analytics/refunds` endpoint (see the
// Analytics & Reports plan); `net_refund_cents` is the P&L figure the summary
// and monthly-profit reports use today.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RefundTotals {
    /// `Σ net_refund_cents` — the total value credited back to customers.
    pub net_refund_cents: i64,
    /// `Σ refund_cash_cents` — the portion actually paid out.
    pub refund_cash_cents: i64,
    /// `Σ balance_reduction_cents` — the portion written off an invoice balance.
    pub balance_reduction_cents: i64,
    /// Number of credit notes counted.
    pub credit_note_count: u64,
}

pub(crate) async fn aggregate_refund_totals(
    db: &Database,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<RefundTotals> {
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
