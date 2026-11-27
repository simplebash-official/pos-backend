//! The slip a customer gets for the jobs they leave with the shop: which jobs may share one,
//! what each says, and what they come to together. Reading tickets and rendering are the
//! caller's job.

use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::domain::print_jobs::PrintJob;
use crate::domain::print_jobs::PrintJobFrame;
use crate::domain::repairs::Repair;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashSet;
use utoipa::{IntoParams, ToSchema};

/// The most jobs one slip lists.
pub const MAX_RECEIPT_JOBS: usize = 20;

/// The paper a job receipt is printed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
pub enum JobReceiptPaper {
    /// A5 sheet, for an office printer.
    #[serde(rename = "a5")]
    A5,
    /// 58 mm receipt roll.
    #[serde(rename = "thermal-58")]
    Thermal58,
    /// 80 mm receipt roll.
    #[serde(rename = "thermal-80")]
    Thermal80,
}

impl JobReceiptPaper {
    /// The name this value has on the wire and in the receipt's payload.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            JobReceiptPaper::A5 => "a5",
            JobReceiptPaper::Thermal58 => "thermal-58",
            JobReceiptPaper::Thermal80 => "thermal-80",
        }
    }
}

/// `GET /job-receipts/document`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct JobReceiptQuery {
    /// The keys of 1 to 20 repair and print jobs of one customer, separated by commas, in the
    /// order to print them.
    pub jobs: String,
    /// `a5`, `thermal-58` or `thermal-80`.
    pub paper: JobReceiptPaper,
    /// A design family for this print only; defaults to the shop's choice for job receipts.
    pub template: Option<String>,
}

/// Which kind of job a receipt line is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobReceiptKind {
    Repair,
    Print,
}

impl JobReceiptKind {
    /// The name this value has in the receipt's payload.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            JobReceiptKind::Repair => "repair",
            JobReceiptKind::Print => "print",
        }
    }

    fn key_prefix(self) -> &'static str {
        match self {
            JobReceiptKind::Repair => prefixes::REPAIR,
            JobReceiptKind::Print => prefixes::PRINT_JOB,
        }
    }
}

/// One job asked for on a slip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReceiptKey {
    pub kind: JobReceiptKind,
    pub key: String,
}

impl JobReceiptKey {
    /// Reads the `jobs` parameter.
    ///
    /// # Errors
    ///
    /// A 400 `JOB_RECEIPT_JOBS_INVALID` for no jobs or more than [`MAX_RECEIPT_JOBS`], a repeat,
    /// or a key that is neither a repair's nor a print job's.
    pub fn parse_list(jobs: &str) -> AppResult<Vec<JobReceiptKey>> {
        let invalid = |message: String| {
            AppError::validation_with_code(message, codes::JOB_RECEIPT_JOBS_INVALID)
        };
        let parts: Vec<&str> = jobs.split(',').collect();
        if jobs.is_empty() || parts.len() > MAX_RECEIPT_JOBS {
            return Err(invalid(format!(
                "A job slip lists 1 to {MAX_RECEIPT_JOBS} jobs."
            )));
        }
        let mut seen = HashSet::new();
        let mut keys = Vec::with_capacity(parts.len());
        for part in parts {
            let kind = [JobReceiptKind::Repair, JobReceiptKind::Print]
                .into_iter()
                .find(|kind| {
                    part.strip_prefix(kind.key_prefix())
                        .is_some_and(|rest| rest.starts_with('_'))
                })
                .ok_or_else(|| invalid(format!("`{part}` is not a repair or print job key.")))?;
            if !seen.insert(part) {
                return Err(invalid(format!("`{part}` is on the slip twice.")));
            }
            keys.push(JobReceiptKey {
                kind,
                key: part.to_string(),
            });
        }
        Ok(keys)
    }
}

/// Who a slip is for, as a ticket names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReceiptCustomer {
    pub key: String,
    pub name: String,
    pub phone: String,
}

/// What a slip says about one job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReceiptJob {
    pub kind: JobReceiptKind,
    pub ticket_number: String,
    pub received_at: DateTime<Utc>,
    /// Repair: the device. Print job: the kind of print.
    pub title: String,
    /// Repair: what the customer reported. Print job: the quantity and kind of print.
    pub detail: String,
    pub serial_number: Option<String>,
    /// What the shop plans to do, one line per item; a print job lists the frames the customer
    /// wants first, and each of its lines says its quantity.
    pub item_lines: Vec<String>,
    /// The estimate in cents; none while nothing is quoted.
    pub estimate_cents: Option<i64>,
    /// The promised ready day, `YYYY-MM-DD`.
    pub promised_ready_on: Option<String>,
    pub is_warranty_rework: bool,
}

/// One ticket's part of a slip, before the slip is put together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReceiptSource {
    pub customer: JobReceiptCustomer,
    pub job: JobReceiptJob,
}

/// The print job type as the receipt names it. The stored type is one of a fixed set.
fn print_job_title(job_type: &str) -> AppResult<&'static str> {
    match job_type {
        "mug" => Ok("Mug"),
        "t-shirt" => Ok("T-shirt"),
        "handbill" => Ok("Handbill"),
        "banner" => Ok("Banner"),
        "photo-framing" => Ok("Photo & Framing"),
        "custom" => Ok("Custom print"),
        other => Err(AppError::internal(format!(
            "print job is stored with the unknown type `{other}`"
        ))),
    }
}

impl JobReceiptSource {
    /// A repair job on a slip. A job with no work on it yet has no estimate, and a warranty
    /// rework is free.
    #[must_use]
    pub fn from_repair(repair: &Repair) -> Self {
        let is_warranty_rework = repair.kind.is_rework();
        let estimate_cents = if repair.items.is_empty() || is_warranty_rework {
            None
        } else {
            Some(repair.price_cents)
        };
        JobReceiptSource {
            customer: JobReceiptCustomer {
                key: repair.customer_key.clone(),
                name: repair.customer_name.clone(),
                phone: repair.customer_phone.clone(),
            },
            job: JobReceiptJob {
                kind: JobReceiptKind::Repair,
                ticket_number: repair.ticket_number.clone(),
                received_at: repair.created_at,
                title: repair.device_model.clone(),
                detail: repair.issue_description.clone(),
                serial_number: repair.serial_number.clone(),
                item_lines: repair
                    .items
                    .iter()
                    .map(|item| item.description.clone())
                    .collect(),
                estimate_cents,
                promised_ready_on: repair.promised_ready_at.clone(),
                is_warranty_rework,
            },
        }
    }

    /// A print job on a slip. A job with no items yet has no estimate, and a warranty rework is
    /// free. The staff note is never printed.
    ///
    /// # Errors
    ///
    /// An internal error when the stored job type is not one the shop can choose.
    pub fn from_print_job(job: &PrintJob) -> AppResult<Self> {
        let title = print_job_title(&job.job_type)?;
        let is_warranty_rework = job.kind.is_rework();
        let estimate_cents = if job.items.is_empty() || is_warranty_rework {
            None
        } else {
            Some(job.price_cents)
        };
        Ok(JobReceiptSource {
            customer: JobReceiptCustomer {
                key: job.customer_key.clone(),
                name: job.customer_name.clone(),
                phone: job.customer_phone.clone(),
            },
            job: JobReceiptJob {
                kind: JobReceiptKind::Print,
                ticket_number: job.ticket_number.clone(),
                received_at: job.created_at,
                title: title.to_string(),
                detail: format!("{} × {title}", job.quantity),
                serial_number: None,
                item_lines: job
                    .frames
                    .iter()
                    .map(PrintJobFrame::receipt_line)
                    .chain(
                        job.items
                            .iter()
                            .map(|item| format!("{} x {}", item.description, item.quantity)),
                    )
                    .collect(),
                estimate_cents,
                promised_ready_on: job.promised_ready_at.clone(),
                is_warranty_rework,
            },
        })
    }
}

/// One slip: a customer and the jobs on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReceipt {
    /// The customer, named as on the first job.
    pub customer: JobReceiptCustomer,
    pub jobs: Vec<JobReceiptJob>,
    /// The estimates added up; none for a single job, while a job that will be charged has no
    /// estimate, and when no job on the slip is charged.
    pub estimate_total_cents: Option<i64>,
}

impl JobReceipt {
    /// Puts the jobs of one customer on one slip, in the order given.
    ///
    /// # Errors
    ///
    /// A 400 `JOB_RECEIPT_CUSTOMERS_DIFFER` when the jobs belong to different customers, and an
    /// internal error for no jobs or estimates too large to add.
    pub fn new(sources: Vec<JobReceiptSource>) -> AppResult<Self> {
        let mut customer: Option<JobReceiptCustomer> = None;
        let mut jobs = Vec::with_capacity(sources.len());
        for source in sources {
            match &customer {
                None => customer = Some(source.customer),
                Some(first) if first.key == source.customer.key => {}
                Some(_) => {
                    return Err(AppError::validation_with_code(
                        "A job slip lists the jobs of one customer.",
                        codes::JOB_RECEIPT_CUSTOMERS_DIFFER,
                    ));
                }
            }
            jobs.push(source.job);
        }
        let customer =
            customer.ok_or_else(|| AppError::internal("a job slip was built with no jobs"))?;
        let estimate_total_cents = total_estimate(&jobs)?;
        Ok(JobReceipt {
            customer,
            jobs,
            estimate_total_cents,
        })
    }
}

fn total_estimate(jobs: &[JobReceiptJob]) -> AppResult<Option<i64>> {
    let charged: Vec<Option<i64>> = jobs
        .iter()
        .filter(|job| !job.is_warranty_rework)
        .map(|job| job.estimate_cents)
        .collect();
    if jobs.len() < 2 || charged.is_empty() {
        return Ok(None);
    }
    let mut total: i64 = 0;
    for estimate in charged {
        let Some(estimate) = estimate else {
            return Ok(None);
        };
        total = total
            .checked_add(estimate)
            .ok_or_else(|| AppError::internal("the estimates on a job slip are too large"))?;
    }
    Ok(Some(total))
}

#[cfg(test)]
mod tests {
    use super::{JobReceiptKey, JobReceiptKind, MAX_RECEIPT_JOBS, print_job_title};

    #[test]
    fn photo_framing_is_named_on_the_receipt() {
        assert_eq!(
            print_job_title("photo-framing").ok(),
            Some("Photo & Framing")
        );
    }

    #[test]
    fn the_jobs_parameter_names_both_kinds_in_order() {
        let keys = JobReceiptKey::parse_list("prn_b,rep_a").expect("two jobs");
        let read: Vec<(JobReceiptKind, &str)> = keys
            .iter()
            .map(|key| (key.kind, key.key.as_str()))
            .collect();
        assert_eq!(
            read,
            [
                (JobReceiptKind::Print, "prn_b"),
                (JobReceiptKind::Repair, "rep_a")
            ]
        );
    }

    #[test]
    fn the_jobs_parameter_refuses_what_is_not_one_to_twenty_distinct_jobs() {
        let too_many: Vec<String> = (0..=MAX_RECEIPT_JOBS).map(|n| format!("rep_{n}")).collect();
        let too_many = too_many.join(",");
        for refused in [
            "",
            "rep_a,",
            "rep_a,rep_a",
            "inv_a",
            "repa",
            " rep_a",
            &too_many,
        ] {
            assert!(JobReceiptKey::parse_list(refused).is_err(), "{refused:?}");
        }
    }
}
