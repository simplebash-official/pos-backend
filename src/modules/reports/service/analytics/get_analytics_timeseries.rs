use super::super::dates;
use super::super::dates::report_tz;
use super::bps::bps;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::{
    AnalyticsRangeQuery, Granularity, TimeSeriesPoint, TimeSeriesResponse,
};
use crate::modules::reports::repository;
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Utc};

fn resolve_granularity(
    explicit: Option<&str>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Granularity> {
    match explicit.map(|g| g.to_lowercase()).as_deref() {
        Some("day") => Ok(Granularity::Day),
        Some("week") => Ok(Granularity::Week),
        Some("month") => Ok(Granularity::Month),
        Some("year") => Ok(Granularity::Year),
        Some(other) => Err(crate::core::error::AppError::validation(format!(
            "Unknown granularity '{other}'. Use day, week, month or year."
        ))),
        None => {
            let days = (end - start).num_days();
            Ok(if days <= 31 {
                Granularity::Day
            } else if days <= 366 {
                Granularity::Week
            } else if days <= 1096 {
                Granularity::Month
            } else {
                Granularity::Year
            })
        }
    }
}

/// Align an instant down to the start of its bucket, in shop-local time.
fn align_bucket(instant: DateTime<Utc>, gran: Granularity, tz: FixedOffset) -> DateTime<Utc> {
    let local = instant.with_timezone(&tz);
    let date = local.date_naive();
    let aligned_date = match gran {
        Granularity::Day => date,
        Granularity::Week => date - Duration::days(date.weekday().num_days_from_monday() as i64),
        Granularity::Month => NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
            .expect("the first of a month is a valid date"),
        Granularity::Year => NaiveDate::from_ymd_opt(date.year(), 1, 1)
            .expect("the first of a month is a valid date"),
    };
    aligned_date
        .and_hms_opt(0, 0, 0)
        .expect("midnight is a valid time")
        .and_local_timezone(tz)
        .single()
        .expect("a fixed offset has no ambiguous local time")
        .with_timezone(&Utc)
}

fn next_bucket(bucket_start: DateTime<Utc>, gran: Granularity, tz: FixedOffset) -> DateTime<Utc> {
    let local = bucket_start.with_timezone(&tz);
    let date = local.date_naive();
    let next_date = match gran {
        Granularity::Day => date + Duration::days(1),
        Granularity::Week => date + Duration::days(7),
        Granularity::Month => {
            if date.month() == 12 {
                NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
                    .expect("the first of a month is a valid date")
            } else {
                NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
                    .expect("the first of a month is a valid date")
            }
        }
        Granularity::Year => NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
            .expect("the first of a month is a valid date"),
    };
    next_date
        .and_hms_opt(0, 0, 0)
        .expect("midnight is a valid time")
        .and_local_timezone(tz)
        .single()
        .expect("a fixed offset has no ambiguous local time")
        .with_timezone(&Utc)
}

fn bucket_label(bucket_start: DateTime<Utc>, gran: Granularity, tz: FixedOffset) -> String {
    let local = bucket_start.with_timezone(&tz);
    match gran {
        Granularity::Day => local.format("%-d %b").to_string(),
        Granularity::Week => local.format("%-d %b").to_string(),
        Granularity::Month => local.format("%b %Y").to_string(),
        Granularity::Year => local.format("%Y").to_string(),
    }
}

// ---------------------------------------------------------------------------
// timeseries
// ---------------------------------------------------------------------------
pub(crate) async fn get_analytics_timeseries(
    db: &Db,
    query: AnalyticsRangeQuery,
) -> AppResult<TimeSeriesResponse> {
    let (start, end) = dates::parse_date_range_tz(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;
    let gran = resolve_granularity(query.granularity.as_deref(), start, end)?;
    let tz = report_tz();

    let (raw, commission) = tokio::try_join!(
        repository::analytics::aggregate_revenue_series(db, start, end, gran),
        repository::analytics::aggregate_commission_series(db, start, end, gran),
    )?;

    let raw_by_ms: std::collections::BTreeMap<i64, _> = raw
        .into_iter()
        .map(|b| (b.period_start.timestamp_millis(), b))
        .collect();

    // Determine the actual starting bucket for the timeseries:
    // If raw or commission data exists, find the earliest period containing data.
    // If start is before that earliest period, align cursor to that earliest period
    // so we never produce dozens of empty leading zero buckets (e.g. from 1970).
    let earliest_data_ts = raw_by_ms
        .keys()
        .next()
        .copied()
        .or_else(|| commission.keys().next().copied());

    let effective_start = if let Some(earliest_ms) = earliest_data_ts {
        let earliest_dt = DateTime::from_timestamp_millis(earliest_ms).unwrap_or(start);
        if earliest_dt > start {
            align_bucket(earliest_dt, gran, tz)
        } else {
            start
        }
    } else {
        start
    };

    let now_local = Utc::now().with_timezone(&tz);
    let today = now_local.date_naive();
    let today_bound = today
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is valid")
        .and_local_timezone(tz)
        .single()
        .expect("fixed offset has no ambiguous local times")
        .with_timezone(&Utc)
        + Duration::days(1);
    let effective_end = end.min(today_bound);

    let mut points = Vec::new();
    let mut cursor = align_bucket(effective_start, gran, tz);
    // Guard against a pathological range producing an unbounded loop.
    let mut guard = 0;
    while cursor < effective_end && guard < 5_000 {
        guard += 1;
        let ms = cursor.timestamp_millis();
        let commission_cents = commission.get(&ms).copied().unwrap_or(0);

        let point = if let Some(b) = raw_by_ms.get(&ms) {
            let revenue = b.retail_revenue_cents + b.repair_revenue_cents + b.print_revenue_cents;
            let gross_profit = revenue - b.cogs_cents;
            TimeSeriesPoint {
                period_start: cursor,
                label: bucket_label(cursor, gran, tz),
                revenue_cents: revenue,
                retail_revenue_cents: b.retail_revenue_cents,
                repair_revenue_cents: b.repair_revenue_cents,
                print_revenue_cents: b.print_revenue_cents,
                discount_cents: b.discount_cents,
                cogs_cents: b.cogs_cents,
                gross_profit_cents: gross_profit,
                gross_margin_bps: bps(gross_profit, revenue),
                commission_cents,
                net_profit_cents: gross_profit - commission_cents,
                invoice_count: b.invoice_count,
            }
        } else {
            TimeSeriesPoint {
                period_start: cursor,
                label: bucket_label(cursor, gran, tz),
                commission_cents,
                net_profit_cents: -commission_cents,
                ..Default::default()
            }
        };
        points.push(point);
        cursor = next_bucket(cursor, gran, tz);
    }

    let totals = fold_totals(&points);

    Ok(TimeSeriesResponse {
        granularity: gran.as_mongo_unit().to_string(),
        period_start: start,
        period_end: end,
        points,
        totals,
    })
}

fn fold_totals(points: &[TimeSeriesPoint]) -> TimeSeriesPoint {
    let mut t = TimeSeriesPoint {
        label: "Total".to_string(),
        ..Default::default()
    };
    for p in points {
        t.revenue_cents += p.revenue_cents;
        t.retail_revenue_cents += p.retail_revenue_cents;
        t.repair_revenue_cents += p.repair_revenue_cents;
        t.print_revenue_cents += p.print_revenue_cents;
        t.discount_cents += p.discount_cents;
        t.cogs_cents += p.cogs_cents;
        t.gross_profit_cents += p.gross_profit_cents;
        t.commission_cents += p.commission_cents;
        t.net_profit_cents += p.net_profit_cents;
        t.invoice_count += p.invoice_count;
    }
    t.gross_margin_bps = bps(t.gross_profit_cents, t.revenue_cents);
    if let Some(first) = points.first() {
        t.period_start = first.period_start;
    }
    t
}
