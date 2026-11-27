//! The payload of the "Job Receipt" template: the slip a customer gets for the repair and print
//! jobs they leave with the shop. Only builds data; reading the tickets and rendering are the
//! callers' job.

use crate::core::error::{AppError, AppResult};
use crate::core::utils::shop_local;
use crate::domain::job_receipts::{JobReceipt, JobReceiptJob, JobReceiptPaper};
use crate::domain::settings::ShopProfileResponse;
use chrono::NaiveDate;
use serde_json::{Value, json};

/// A stored `YYYY-MM-DD` day as printed (`27 Nov 2026`).
fn printed_day(day: &str) -> AppResult<String> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map(|date| date.format("%d %b %Y").to_string())
        .map_err(|err| {
            AppError::internal(format!(
                "stored promised-ready day `{day}` is invalid: {err}"
            ))
        })
}

fn job_data(job: &JobReceiptJob) -> AppResult<Value> {
    let received = shop_local(job.received_at);
    let promised_ready_on = match &job.promised_ready_on {
        Some(day) => json!(printed_day(day)?),
        None => Value::Null,
    };
    Ok(json!({
        "ticketNumber": job.ticket_number,
        "jobKind": job.kind.as_str(),
        "receivedOn": received.format("%d %b %Y").to_string(),
        "receivedAt": received.format("%H:%M").to_string(),
        "title": job.title,
        "detail": job.detail,
        "serialNumber": job.serial_number.clone().unwrap_or_default(),
        "itemLines": job.item_lines,
        "estimateCents": job.estimate_cents,
        "promisedReadyOn": promised_ready_on,
        "isWarrantyRework": job.is_warranty_rework,
        "qrData": job.ticket_number,
    }))
}

/// Builds `data` for the "Job Receipt" template. The slip shows the jobs as they are now, in the
/// shop's own time zone, with the shop's details, terms and pickup warning from its profile.
///
/// # Errors
///
/// An internal error when a stored promised-ready day cannot be read.
pub(crate) fn build_job_receipt_data(
    receipt: &JobReceipt,
    shop: &ShopProfileResponse,
    paper: JobReceiptPaper,
) -> AppResult<Value> {
    let jobs = receipt
        .jobs
        .iter()
        .map(job_data)
        .collect::<AppResult<Vec<_>>>()?;
    Ok(json!({
        "paper": paper.as_str(),
        "logoUrl": shop.logo_base64,
        "customerName": receipt.customer.name,
        "customerPhone": receipt.customer.phone,
        "jobs": jobs,
        "estimateTotalCents": receipt.estimate_total_cents,
        "termsText": shop.job_receipt_terms_text,
        "uncollectedAfterDays": shop.uncollected_after_days,
        "shopTradingName": shop.trading_name,
        "shopLegalName": shop.legal_name,
        "shopAddressLines": shop.address_lines,
        "shopPrimaryPhone": shop.primary_phone,
        "shopSecondaryPhone": shop.secondary_phone,
        "shopEmail": shop.email,
        "shopWebsite": shop.website,
    }))
}
