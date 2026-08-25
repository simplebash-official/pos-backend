// MongoDB-only aggregation of technician & staff commission figures across
// repairs and print jobs. Grouped by `assigned_employee_id` (a real
// `modules::employees` key), never by the free-text `assigned_employee_name`
// display string — two employees sharing a name must not merge into one
// entry, and renaming an employee must not split their history across two
// buckets. Display name/role resolution against `employees` is deliberately
// NOT done here — this file is Mongo-only, per this codebase's "reach
// another module through its service, never its repository" rule — see
// `service::commissions::get_employee_commissions_report`, which calls
// `employees::service::get_employees_by_keys` after this function returns.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc},
};

use crate::{core::error::AppResult, domain::reports::EmployeePerformanceEntry};

/// Internal bucket keys for jobs with no `assigned_employee_id` at all —
/// never resolved against `employees` and never treated as a real
/// employee's key by the caller.
pub(crate) const UNASSIGNED_TECHNICIAN: &str = "__unassigned_technician__";
pub(crate) const UNASSIGNED_PRINTER: &str = "__unassigned_printer__";

fn repairs(db: &Database) -> Collection<Document> {
    db.collection("repairs")
}

fn print_jobs(db: &Database) -> Collection<Document> {
    db.collection("print_jobs")
}

pub(crate) fn get_i64_flexible(doc: &Document, key: &str) -> i64 {
    doc.get_i64(key)
        .ok()
        .or_else(|| doc.get_i32(key).ok().map(i64::from))
        .or_else(|| doc.get_f64(key).ok().map(|v| v as i64))
        .unwrap_or(0)
}

/// Shared commission formula — reused by `employee_earnings`'s itemized
/// per-ticket listing so the two views can never silently disagree on how a
/// commission is computed.
pub(crate) fn compute_earned_cents(
    profit_cents: i64,
    split_type: Option<&str>,
    split_value: f64,
) -> i64 {
    match split_type {
        Some("percentage") => ((profit_cents as f64 * split_value) / 100.0).round() as i64,
        Some("fixed") => split_value.round() as i64,
        _ => 0,
    }
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
) {
    let bucket_key = doc
        .get_str("assigned_employee_id")
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| unassigned_sentinel.to_string());

    let revenue = get_i64_flexible(doc, "estimated_cost_cents");
    let material_cost = get_i64_flexible(doc, "material_cost_cents");
    let profit = (revenue - material_cost).max(0);

    let split_type = doc.get_str("split_type").ok();
    let split_val = doc
        .get_f64("split_value")
        .ok()
        .or_else(|| doc.get_i64("split_value").ok().map(|v| v as f64))
        .or_else(|| doc.get_i32("split_value").ok().map(|v| v as f64))
        .unwrap_or(0.0);

    let earned = compute_earned_cents(profit, split_type, split_val);
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
}

/// Returns the accumulated per-bucket entries keyed by
/// `assigned_employee_id` (or `UNASSIGNED_TECHNICIAN`/`UNASSIGNED_PRINTER`).
/// `employee_name`/`role` are unresolved (blank) for real employee buckets —
/// the caller must resolve them via `employees::service::get_employees_by_keys`
/// before returning this to an API client.
pub(crate) async fn aggregate_employee_commissions(
    db: &Database,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    filter_employee_key: Option<&str>,
) -> AppResult<BTreeMap<String, EmployeePerformanceEntry>> {
    let mut perf_map: BTreeMap<String, EmployeePerformanceEntry> = BTreeMap::new();

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
        );
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
        );
    }

    Ok(perf_map)
}
