use super::super::super::service::dates::REPORT_TZ_MONGO;

use super::super::sales::get_i64_flexible;
use super::date_match::date_match;
use super::invoices::invoices;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;

use crate::core::error::AppResult;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::doc;

// ---------------------------------------------------------------------------
// sales patterns (weekday x hour)
// ---------------------------------------------------------------------------

/// `(weekday 1..7, hour 0..23, invoice count, revenue)` cells present in the
/// data — the service layer zero-fills the full 7×24 grid.
pub(crate) async fn aggregate_sales_patterns(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Vec<(u8, u8, u64, i64)>> {
    match db {
        Db::Mongo(db) => {
            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! {
                    "$group": {
                        "_id": {
                            "weekday": { "$isoDayOfWeek": { "date": "$created_at", "timezone": REPORT_TZ_MONGO } },
                            "hour": { "$hour": { "date": "$created_at", "timezone": REPORT_TZ_MONGO } },
                        },
                        "count": { "$sum": 1 },
                        "revenue": { "$sum": "$total_cents" },
                    }
                },
            ];
            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut out = Vec::new();
            while let Some(row) = cursor.try_next().await? {
                let Ok(id) = row.get_document("_id") else {
                    continue;
                };
                let weekday = get_i64_flexible(id, "weekday").clamp(1, 7) as u8;
                let hour = get_i64_flexible(id, "hour").clamp(0, 23) as u8;
                out.push((
                    weekday,
                    hour,
                    get_i64_flexible(&row, "count") as u64,
                    get_i64_flexible(&row, "revenue"),
                ));
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            use chrono::{Datelike, FixedOffset, Timelike};
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT created_at, total_cents FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let tz = FixedOffset::east_opt(5 * 3600 + 30 * 60).expect("+05:30 is a valid offset");
            let mut map: std::collections::HashMap<(u8, u8), (u64, i64)> =
                std::collections::HashMap::new();

            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                let created_at = if let Ok(bdt) = doc.get_datetime("created_at") {
                    bdt.to_chrono()
                } else {
                    continue;
                };
                let local = created_at.with_timezone(&tz);
                let weekday = local.weekday().number_from_monday() as u8;
                let hour = local.hour() as u8;
                let rev = get_i64_flexible(&doc, "total_cents");
                let entry = map.entry((weekday, hour)).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += rev;
            }

            let out = map
                .into_iter()
                .map(|((w, h), (cnt, rev))| (w, h, cnt, rev))
                .collect();
            Ok(out)
        }
    }
}
