use crate::core::error::{AppError, AppResult};
use crate::domain::print_jobs::PrintJob;
use crate::domain::repairs::Repair;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

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

/// `GET /repairs/{key}/documents/job-receipt` and `GET /print-jobs/{key}/documents/job-receipt`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct JobReceiptQuery {
    /// `a5`, `thermal-58` or `thermal-80`.
    pub paper: JobReceiptPaper,
    /// A design family for this print only; defaults to the shop's choice for job receipts.
    pub template: Option<String>,
}

/// Which kind of job a receipt is for.
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
}

/// What a job receipt says about the job, taken from a repair or a print job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReceiptSource {
    pub kind: JobReceiptKind,
    /// The ticket's key; the receipt's PDFs are cached under it.
    pub key: String,
    /// The ticket's version; a changed ticket prints afresh.
    pub version: i64,
    pub ticket_number: String,
    pub received_at: DateTime<Utc>,
    pub customer_name: String,
    pub customer_phone: String,
    /// Repair: the device. Print job: the kind of print.
    pub title: String,
    /// Repair: what the customer reported. Print job: the quantity and kind of print.
    pub detail: String,
    pub serial_number: Option<String>,
    /// What the shop plans to do, one line per item; a print job's line says its quantity.
    pub item_lines: Vec<String>,
    /// The estimate in cents; none while nothing is quoted.
    pub estimate_cents: Option<i64>,
    /// The promised ready day, `YYYY-MM-DD`.
    pub promised_ready_on: Option<String>,
    pub is_warranty_rework: bool,
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
    /// The receipt of a repair job. A job with no work on it yet has no estimate, and a warranty
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
            kind: JobReceiptKind::Repair,
            key: repair.key.clone(),
            version: repair.version,
            ticket_number: repair.ticket_number.clone(),
            received_at: repair.created_at,
            customer_name: repair.customer_name.clone(),
            customer_phone: repair.customer_phone.clone(),
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
        }
    }

    /// The receipt of a print job. A job with no items yet has no estimate, and a warranty
    /// rework is free. The staff note is never printed.
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
            kind: JobReceiptKind::Print,
            key: job.key.clone(),
            version: job.version,
            ticket_number: job.ticket_number.clone(),
            received_at: job.created_at,
            customer_name: job.customer_name.clone(),
            customer_phone: job.customer_phone.clone(),
            title: title.to_string(),
            detail: format!("{} × {title}", job.quantity),
            serial_number: None,
            item_lines: job
                .items
                .iter()
                .map(|item| format!("{} x {}", item.description, item.quantity))
                .collect(),
            estimate_cents,
            promised_ready_on: job.promised_ready_at.clone(),
            is_warranty_rework,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::print_job_title;

    #[test]
    fn photo_framing_is_named_on_the_receipt() {
        assert_eq!(
            print_job_title("photo-framing").ok(),
            Some("Photo & Framing")
        );
    }
}
