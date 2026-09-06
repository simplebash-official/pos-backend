// Mongo queries for the dashboard reminders feed: unpaid credit invoices and
// open repair/print jobs whose due / promised date is overdue or coming up.
//
// Kept deliberately simple — a `find` per source plus one batched payments
// lookup, then all the date math in Rust — because the set of open credit
// invoices + open jobs a shop has at any moment is small. Mirrors the style
// of `receivables::list_outstanding_invoices`.

use std::collections::HashMap;

use chrono::{NaiveDate, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc, oid::ObjectId},
};

use sqlx::Row;

use crate::{
    clients::{db::Db, sqlite::map_sqlite_row_to_document},
    core::error::AppResult,
    domain::reports::{ReminderEntry, ReminderKind},
};

fn collection(db: &Database, name: &str) -> Collection<Document> {
    db.collection(name)
}

const CLOSED_JOB_STATUSES: [&str; 2] = ["delivered", "cancelled"];

/// The full reminder set (already merged + sorted, but NOT truncated) plus
/// honest headline counts over that full set.
pub(crate) struct RemindersRaw {
    pub entries: Vec<ReminderEntry>,
    pub credit_overdue_count: u64,
    pub credit_overdue_amount_cents: i64,
    pub credit_due_soon_count: u64,
    pub credit_due_soon_amount_cents: i64,
    pub jobs_overdue_count: u64,
    pub jobs_due_soon_count: u64,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}

/// `positive` = overdue by N days, `negative` = due in N days, `0` = today.
fn days_from_due(today: NaiveDate, date_str: &str) -> Option<i64> {
    let due = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
    Some((today - due).num_days())
}

/// Which bucket a `days_from_due` value falls in, given the "due soon"
/// lookahead window. Returns `None` for dates too far in the future to care
/// about yet.
fn classify(days: i64, due_within_days: i64) -> Option<Bucket> {
    if days > 0 {
        Some(Bucket::Overdue)
    } else if days >= -due_within_days {
        Some(Bucket::DueSoon)
    } else {
        None
    }
}

enum Bucket {
    Overdue,
    DueSoon,
}

pub(crate) async fn list_reminders(
    db: &Db,
    due_within_days: u32,
    page: u64,
    limit: u64,
    skip: u64,
) -> AppResult<RemindersRaw> {
    let today = Utc::now().date_naive();
    let window = due_within_days as i64;

    let mut overdue: Vec<ReminderEntry> = Vec::new();
    let mut due_soon: Vec<ReminderEntry> = Vec::new();

    let mut raw = RemindersRaw {
        entries: Vec::new(),
        credit_overdue_count: 0,
        credit_overdue_amount_cents: 0,
        credit_due_soon_count: 0,
        credit_due_soon_amount_cents: 0,
        jobs_overdue_count: 0,
        jobs_due_soon_count: 0,
        total: 0,
        page,
        limit,
        total_pages: 1,
    };

    let (invoice_docs, paid_by_invoice, repair_docs, print_job_docs) = match db {
        Db::Mongo(db) => {
            let mut invoice_cursor = collection(db, "invoices")
                .find(doc! {
                    "status": { "$in": ["pending", "partially_paid"] },
                    "due_date": { "$ne": null },
                })
                .await?;
            let mut invoice_docs = Vec::new();
            let mut invoice_keys = Vec::new();
            while let Some(doc) = invoice_cursor.try_next().await? {
                if let Ok(k) = doc.get_str("key") {
                    invoice_keys.push(k.to_string());
                }
                invoice_docs.push(doc);
            }

            let mut paid_by_invoice: HashMap<String, i64> = HashMap::new();
            if !invoice_keys.is_empty() {
                let mut p_cursor = collection(db, "payments")
                    .find(doc! { "invoice_key": { "$in": &invoice_keys } })
                    .await?;
                while let Some(p) = p_cursor.try_next().await? {
                    if let (Ok(ik), Ok(amt)) = (p.get_str("invoice_key"), p.get_i64("amount_cents"))
                    {
                        *paid_by_invoice.entry(ik.to_string()).or_insert(0) += amt;
                    }
                }
            }

            let mut repair_docs = Vec::new();
            let mut r_cursor = collection(db, "repairs")
                .find(doc! {
                    "promised_ready_at": { "$ne": null },
                    "status": { "$nin": CLOSED_JOB_STATUSES.to_vec() },
                    "deleted_at": { "$exists": false },
                })
                .await?;
            while let Some(doc) = r_cursor.try_next().await? {
                repair_docs.push(doc);
            }

            let mut print_job_docs = Vec::new();
            let mut pj_cursor = collection(db, "print_jobs")
                .find(doc! {
                    "promised_ready_at": { "$ne": null },
                    "status": { "$nin": CLOSED_JOB_STATUSES.to_vec() },
                    "deleted_at": { "$exists": false },
                })
                .await?;
            while let Some(doc) = pj_cursor.try_next().await? {
                print_job_docs.push(doc);
            }

            (invoice_docs, paid_by_invoice, repair_docs, print_job_docs)
        }
        Db::Sqlite(pool) => {
            let inv_rows = sqlx::query(
                "SELECT * FROM invoices WHERE status IN ('pending', 'partially_paid') AND due_date IS NOT NULL",
            )
            .fetch_all(pool)
            .await?;
            let mut invoice_docs = Vec::new();
            for r in inv_rows {
                invoice_docs.push(map_sqlite_row_to_document(&r));
            }

            let p_rows = sqlx::query(
                "SELECT invoice_key, SUM(amount_cents) as total FROM payments GROUP BY invoice_key",
            )
            .fetch_all(pool)
            .await?;
            let mut paid_by_invoice = HashMap::new();
            for r in p_rows {
                let ik: String = r.get("invoice_key");
                let tot: i64 = r.get("total");
                paid_by_invoice.insert(ik, tot);
            }

            let rep_rows = sqlx::query(
                "SELECT * FROM repairs WHERE promised_ready_at IS NOT NULL AND status NOT IN ('delivered', 'cancelled') AND deleted_at IS NULL",
            )
            .fetch_all(pool)
            .await?;
            let repair_docs: Vec<Document> =
                rep_rows.iter().map(map_sqlite_row_to_document).collect();

            let pj_rows = sqlx::query(
                "SELECT * FROM print_jobs WHERE promised_ready_at IS NOT NULL AND status NOT IN ('delivered', 'cancelled') AND deleted_at IS NULL",
            )
            .fetch_all(pool)
            .await?;
            let print_job_docs: Vec<Document> =
                pj_rows.iter().map(map_sqlite_row_to_document).collect();

            (invoice_docs, paid_by_invoice, repair_docs, print_job_docs)
        }
    };

    for doc in invoice_docs {
        let Ok(due_date) = doc.get_str("due_date") else {
            continue;
        };
        let Some(days) = days_from_due(today, due_date) else {
            continue;
        };
        let Some(bucket) = classify(days, window) else {
            continue;
        };
        let key = doc.get_str("key").unwrap_or_default().to_string();
        let total_cents = doc.get_i64("total_cents").unwrap_or(0);
        let paid = paid_by_invoice.get(&key).copied().unwrap_or(0);
        let balance_due = (total_cents - paid).max(0);
        if balance_due <= 0 {
            continue;
        }
        let customer_name = doc.get_str("customer_name_snapshot").ok().map(String::from);
        let entry = ReminderEntry {
            kind: match bucket {
                Bucket::Overdue => ReminderKind::CreditOverdue,
                Bucket::DueSoon => ReminderKind::CreditDueSoon,
            },
            id: doc
                .get_object_id("_id")
                .map(|o: ObjectId| o.to_hex())
                .unwrap_or_default(),
            key: key.clone(),
            reference_number: doc
                .get_str("invoice_number")
                .unwrap_or_default()
                .to_string(),
            title: customer_name
                .clone()
                .unwrap_or_else(|| "Customer".to_string()),
            customer_name,
            customer_phone: doc
                .get_str("customer_phone_snapshot")
                .ok()
                .map(String::from),
            amount_cents: balance_due,
            due_date: due_date.to_string(),
            days_from_due: days,
            severity: match bucket {
                Bucket::Overdue if days > 7 => "critical",
                Bucket::Overdue => "warning",
                Bucket::DueSoon => "info",
            }
            .to_string(),
            link_to: format!("/invoices?invoiceKey={key}&action=recordPayment"),
        };
        match bucket {
            Bucket::Overdue => {
                raw.credit_overdue_count += 1;
                raw.credit_overdue_amount_cents += balance_due;
                overdue.push(entry);
            }
            Bucket::DueSoon => {
                raw.credit_due_soon_count += 1;
                raw.credit_due_soon_amount_cents += balance_due;
                due_soon.push(entry);
            }
        }
    }

    // ---- Repair & print jobs ---------------------------------------------
    for (docs, route, is_repair) in [
        (repair_docs, "/repairs", true),
        (print_job_docs, "/print-jobs", false),
    ] {
        for doc in docs {
            let Ok(promised) = doc.get_str("promised_ready_at") else {
                continue;
            };
            let Some(days) = days_from_due(today, promised) else {
                continue;
            };
            let Some(bucket) = classify(days, window) else {
                continue;
            };
            let key = doc.get_str("key").unwrap_or_default().to_string();
            let estimated = doc.get_i64("estimated_cost_cents").unwrap_or(0);
            let material = if is_repair {
                doc.get_i64("material_cost_cents").unwrap_or(0)
            } else {
                0
            };
            let title = if is_repair {
                doc.get_str("device_model").unwrap_or_default().to_string()
            } else {
                let job_type = doc.get_str("job_type").unwrap_or_default();
                let qty = doc.get_i32("quantity").unwrap_or(0);
                format!("{job_type} x{qty}")
            };
            let customer_name = doc.get_str("customer_name").ok().map(String::from);
            let entry = ReminderEntry {
                kind: match bucket {
                    Bucket::Overdue => ReminderKind::JobOverdue,
                    Bucket::DueSoon => ReminderKind::JobDueSoon,
                },
                id: doc
                    .get_object_id("_id")
                    .map(|o: ObjectId| o.to_hex())
                    .unwrap_or_default(),
                key: key.clone(),
                reference_number: doc.get_str("ticket_number").unwrap_or_default().to_string(),
                title,
                customer_name,
                customer_phone: doc.get_str("customer_phone").ok().map(String::from),
                amount_cents: estimated + material,
                due_date: promised.to_string(),
                days_from_due: days,
                severity: match bucket {
                    Bucket::Overdue if days >= 3 => "critical",
                    Bucket::Overdue => "warning",
                    Bucket::DueSoon => "info",
                }
                .to_string(),
                link_to: format!("{route}?jobKey={key}"),
            };
            match bucket {
                Bucket::Overdue => {
                    raw.jobs_overdue_count += 1;
                    overdue.push(entry);
                }
                Bucket::DueSoon => {
                    raw.jobs_due_soon_count += 1;
                    due_soon.push(entry);
                }
            }
        }
    }

    // Payment overdues first, then most overdue at the top;
    // then due-soon (payment due soon first, then soonest first).
    overdue.sort_by(|a, b| {
        let is_a_credit = a.kind == ReminderKind::CreditOverdue;
        let is_b_credit = b.kind == ReminderKind::CreditOverdue;
        match (is_a_credit, is_b_credit) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.days_from_due.cmp(&a.days_from_due),
        }
    });
    due_soon.sort_by(|a, b| {
        let is_a_credit = a.kind == ReminderKind::CreditDueSoon;
        let is_b_credit = b.kind == ReminderKind::CreditDueSoon;
        match (is_a_credit, is_b_credit) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.days_from_due.cmp(&a.days_from_due),
        }
    });

    let mut all_entries = overdue;
    all_entries.extend(due_soon);

    let total = all_entries.len() as u64;
    let total_pages = if total == 0 { 1 } else { total.div_ceil(limit) };

    let paged_entries = all_entries
        .into_iter()
        .skip(skip as usize)
        .take(limit as usize)
        .collect();

    raw.entries = paged_entries;
    raw.total = total;
    raw.total_pages = total_pages;

    Ok(raw)
}
