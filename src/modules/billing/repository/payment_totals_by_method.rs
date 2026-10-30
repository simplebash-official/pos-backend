use super::invoices::invoices;
use super::payments::payments;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use futures_util::TryStreamExt;

use mongodb::bson::{DateTime as BsonDateTime, doc};
use sqlx::Row;

/// What customers paid, by method, from `since` (`YYYY-MM-DD`, UTC) on, as
/// `(method, received, refunded)`. A refund is a negative payment row; payments on voided invoices
/// and deleted payments are left out, like the shift totals.
///
/// shortcut: the Mongo side sums in memory over the payments of the range; a shop's payments fit,
/// upgrade to a `$group` pipeline when a shop passes hundreds of thousands.
pub(crate) async fn payment_totals_by_method(
    db: &Db,
    since: &str,
) -> AppResult<Vec<(String, i64, i64)>> {
    let mut totals: std::collections::BTreeMap<String, (i64, i64)> =
        std::collections::BTreeMap::new();
    let mut add = |method: String, amount: i64| {
        let entry = totals.entry(method).or_default();
        if amount >= 0 {
            entry.0 += amount;
        } else {
            entry.1 += -amount;
        }
    };
    match db {
        Db::Mongo(db) => {
            let from = chrono::NaiveDate::parse_from_str(since, "%Y-%m-%d")
                .map_err(|_| crate::core::error::AppError::validation("Invalid date."))?
                .and_hms_opt(0, 0, 0)
                .expect("midnight exists")
                .and_utc();
            let voided: std::collections::HashSet<String> = {
                let mut cursor = invoices(db)
                    .find(
                        doc! { "$or": [{ "status": "voided" }, { "voided_at": { "$ne": null } }] },
                    )
                    .await?;
                let mut keys = std::collections::HashSet::new();
                while let Some(invoice) = cursor.try_next().await? {
                    keys.insert(invoice.key);
                }
                keys
            };
            let mut cursor = payments(db)
                .find(doc! { "deleted_at": null, "recorded_at": { "$gte": BsonDateTime::from_chrono(from) } })
                .await?;
            while let Some(p) = cursor.try_next().await? {
                if !voided.contains(&p.invoice_key) {
                    add(p.payment_method, p.amount_cents);
                }
            }
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT payment_method, amount_cents FROM payments
                 WHERE deleted_at IS NULL AND recorded_at >= ?
                   AND invoice_key NOT IN (SELECT key FROM invoices WHERE status = 'voided' OR voided_at IS NOT NULL)",
            )
            .bind(since)
            .fetch_all(pool)
            .await?;
            for row in rows {
                add(row.get("payment_method"), row.get("amount_cents"));
            }
        }
    }
    Ok(totals
        .into_iter()
        .map(|(method, (received, refunded))| (method, received, refunded))
        .collect())
}
