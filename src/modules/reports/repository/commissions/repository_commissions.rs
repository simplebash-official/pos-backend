use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::calculations::{commission_cents, job_profit_cents};
use crate::core::error::{AppError, AppResult};
use crate::domain::reports::EmployeePerformanceEntry;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{Bson, DateTime as BsonDateTime, Document, doc};
use std::collections::BTreeMap;

fn repairs(db: &TenantDatabase) -> ScopedCollection<Document> {
    db.collection("repairs")
}

fn print_jobs(db: &TenantDatabase) -> ScopedCollection<Document> {
    db.collection("print_jobs")
}

/// Accumulates one repair/print-job document's profit/commission figures
/// into `perf_map`, bucketed by `assigned_employee_id` (or the fixed
/// unassigned sentinel for that job type). `employee_name` is left blank
/// for a real employee bucket — the caller resolves it afterward via a
/// single batch call into `employees::service` — and set immediately for
/// the sentinel bucket, since there's no employee record to resolve.
fn accumulate(
    perf_map: &mut BTreeMap<String, EmployeePerformanceEntry>,
    doc: &Document,
    unassigned_sentinel: &'static str,
    unassigned_label: &'static str,
    role: &'static str,
    kind: JobKind,
) -> AppResult<()> {
    let bucket_key = doc
        .get_str("assigned_employee_id")
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| unassigned_sentinel.to_string());

    let JobMoney {
        revenue_cents: revenue,
        cost_cents: material_cost,
    } = job_money(doc, kind)?;
    let profit = job_profit_cents(revenue, material_cost);

    let split_type = doc.get_str("split_type").ok();
    let split_val = doc
        .get_f64("split_value")
        .ok()
        .or_else(|| doc.get_i64("split_value").ok().map(|v| v as f64))
        .or_else(|| doc.get_i32("split_value").ok().map(|v| v as f64))
        .unwrap_or(0.0);

    let earned = commission_cents(profit, split_type, split_val);
    let net_contribution = (profit - earned).max(0);

    let is_unassigned = bucket_key == unassigned_sentinel;
    let employee_key = if is_unassigned {
        None
    } else {
        Some(bucket_key.clone())
    };
    let entry = perf_map
        .entry(bucket_key)
        .or_insert_with(|| EmployeePerformanceEntry {
            employee_key,
            employee_name: if is_unassigned {
                unassigned_label.to_string()
            } else {
                String::new()
            },
            role: role.to_string(),
            assigned_jobs_count: 0,
            revenue_generated_cents: 0,
            estimated_cost_cents: 0,
            profit_cents: 0,
            earned_commission_cents: 0,
            net_shop_contribution_cents: 0,
        });

    entry.assigned_jobs_count += 1;
    entry.revenue_generated_cents += revenue;
    entry.estimated_cost_cents += material_cost;
    entry.profit_cents += profit;
    entry.earned_commission_cents += earned;
    entry.net_shop_contribution_cents += net_contribution;

    Ok(())
}

/// Returns the accumulated per-bucket entries keyed by
/// `assigned_employee_id` (or `UNASSIGNED_TECHNICIAN`/`UNASSIGNED_PRINTER`).
/// `employee_name`/`role` are unresolved (blank) for real employee buckets —
/// the caller must resolve them via `employees::service::get_employees_by_keys`
/// before returning this to an API client.
pub(crate) async fn aggregate_employee_commissions(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    filter_employee_key: Option<&str>,
) -> AppResult<BTreeMap<String, EmployeePerformanceEntry>> {
    let mut perf_map: BTreeMap<String, EmployeePerformanceEntry> = BTreeMap::new();

    match db {
        Db::Mongo(db) => {
            let mut repair_match = doc! {
                "deleted_at": null,
                "status": { "$in": ["completed", "delivered", "ready_for_pickup"] },
                "created_at": {
                    "$gte": BsonDateTime::from_chrono(start),
                    "$lt": BsonDateTime::from_chrono(end),
                }
            };
            if let Some(key) = filter_employee_key {
                repair_match.insert("assigned_employee_id", key);
            }

            let mut cursor = repairs(db).find(repair_match).await?;
            while let Some(doc) = cursor.try_next().await? {
                accumulate(
                    &mut perf_map,
                    &doc,
                    UNASSIGNED_TECHNICIAN,
                    "Unassigned Technician",
                    "technician",
                    JobKind::Repair,
                )?;
            }

            let mut print_match = doc! {
                "deleted_at": null,
                "status": { "$in": ["completed", "delivered", "ready_for_pickup"] },
                "created_at": {
                    "$gte": BsonDateTime::from_chrono(start),
                    "$lt": BsonDateTime::from_chrono(end),
                }
            };
            if let Some(key) = filter_employee_key {
                print_match.insert("assigned_employee_id", key);
            }

            let mut cursor = print_jobs(db).find(print_match).await?;
            while let Some(doc) = cursor.try_next().await? {
                accumulate(
                    &mut perf_map,
                    &doc,
                    UNASSIGNED_PRINTER,
                    "Unassigned Printer",
                    "printer",
                    JobKind::Print,
                )?;
            }
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();

            let (filter_repair_clause, filter_print_clause) = match filter_employee_key {
                Some(k) => (
                    format!(" AND assigned_employee_id = '{k}'"),
                    format!(" AND assigned_employee_id = '{k}'"),
                ),
                None => ("".to_string(), "".to_string()),
            };

            let repair_query = format!(
                "SELECT * FROM repairs WHERE deleted_at IS NULL AND status IN ('completed', 'delivered', 'ready_for_pickup') AND created_at >= ? AND created_at < ?{filter_repair_clause}"
            );
            let repair_rows = sqlx::query(&repair_query)
                .bind(&start_iso)
                .bind(&end_iso)
                .fetch_all(pool)
                .await?;
            for row in repair_rows {
                let doc = map_sqlite_row_to_document(&row);
                accumulate(
                    &mut perf_map,
                    &doc,
                    UNASSIGNED_TECHNICIAN,
                    "Unassigned Technician",
                    "technician",
                    JobKind::Repair,
                )?;
            }

            let print_query = format!(
                "SELECT * FROM print_jobs WHERE deleted_at IS NULL AND status IN ('completed', 'delivered', 'ready_for_pickup') AND created_at >= ? AND created_at < ?{filter_print_clause}"
            );
            let print_rows = sqlx::query(&print_query)
                .bind(&start_iso)
                .bind(&end_iso)
                .fetch_all(pool)
                .await?;
            for row in print_rows {
                let doc = map_sqlite_row_to_document(&row);
                accumulate(
                    &mut perf_map,
                    &doc,
                    UNASSIGNED_PRINTER,
                    "Unassigned Printer",
                    "printer",
                    JobKind::Print,
                )?;
            }
        }
    }

    Ok(perf_map)
}

/// Which job collection a raw row came from; each keeps its money in its own fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobKind {
    Repair,
    Print,
}

/// What a job charges and what its parts or materials cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JobMoney {
    pub revenue_cents: i64,
    pub cost_cents: i64,
}

/// A job's money from its raw row. A repair stores its items' totals in the required
/// `price_cents`/`cost_cents` (`pos_calc_core::repair`); a print job keeps its quote
/// `estimated_cost_cents` and an optional `material_cost_cents`.
pub(crate) fn job_money(doc: &Document, kind: JobKind) -> AppResult<JobMoney> {
    match kind {
        JobKind::Repair => Ok(JobMoney {
            revenue_cents: get_i64_flexible(doc, "price_cents")?,
            cost_cents: get_i64_flexible(doc, "cost_cents")?,
        }),
        JobKind::Print => Ok(JobMoney {
            revenue_cents: recorded_cents(doc, "estimated_cost_cents")?,
            cost_cents: recorded_cents(doc, "material_cost_cents")?,
        }),
    }
}

/// An amount the document may legitimately not carry (no tender on a credit sale, no estimate on
/// a repair): an absent field counts as nothing recorded, i.e. zero in a sum. A field that is
/// present but not numeric is still an error.
pub(crate) fn recorded_cents(doc: &Document, key: &str) -> AppResult<i64> {
    match doc.get(key) {
        None | Some(Bson::Null) => Ok(0),
        Some(_) => get_i64_flexible(doc, key),
    }
}

/// The numeric field `key` of an aggregation row as an integer; a missing or non-numeric
/// field is an error, not zero.
pub(crate) fn get_i64_flexible(doc: &Document, key: &str) -> AppResult<i64> {
    match doc.get(key) {
        Some(Bson::Int64(v)) => Ok(*v),
        Some(Bson::Int32(v)) => Ok(i64::from(*v)),
        Some(Bson::Double(v)) => Ok(*v as i64),
        _ => Err(AppError::internal(format!(
            "aggregation row has no numeric `{key}`"
        ))),
    }
}

pub(crate) const UNASSIGNED_PRINTER: &str = "__unassigned_printer__";

/// Internal bucket keys for jobs with no `assigned_employee_id` at all —
/// never resolved against `employees` and never treated as a real
/// employee's key by the caller.
pub(crate) const UNASSIGNED_TECHNICIAN: &str = "__unassigned_technician__";
