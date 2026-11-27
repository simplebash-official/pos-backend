//! The payload of the "Job Receipt" template: the slip a customer gets when the shop receives a
//! repair or a print job. Only builds data; reading the ticket and rendering are the callers' job.

use crate::core::error::{AppError, AppResult};
use crate::core::utils::shop_local;
use crate::domain::job_receipts::{JobReceiptPaper, JobReceiptSource};
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

/// Builds `data` for the "Job Receipt" template. The receipt shows the job as it is now, in the
/// shop's own time zone, with the shop's details, terms and pickup warning from its profile.
///
/// # Errors
///
/// An internal error when the stored promised-ready day cannot be read.
pub(crate) fn build_job_receipt_data(
    source: &JobReceiptSource,
    shop: &ShopProfileResponse,
    paper: JobReceiptPaper,
) -> AppResult<Value> {
    let received = shop_local(source.received_at);
    let promised_ready_on = match &source.promised_ready_on {
        Some(day) => json!(printed_day(day)?),
        None => Value::Null,
    };
    let mut data = json!({
        "paper": paper.as_str(),
        "logoUrl": shop.logo_base64,
        "ticketNumber": source.ticket_number,
        "jobKind": source.kind.as_str(),
        "receivedOn": received.format("%d %b %Y").to_string(),
        "receivedAt": received.format("%H:%M").to_string(),
        "customerName": source.customer_name,
        "customerPhone": source.customer_phone,
        "title": source.title,
        "detail": source.detail,
        "serialNumber": source.serial_number.clone().unwrap_or_default(),
        "itemLines": source.item_lines,
        "estimateCents": source.estimate_cents,
        "promisedReadyOn": promised_ready_on,
        "isWarrantyRework": source.is_warranty_rework,
        "qrData": source.ticket_number,
        "termsText": shop.job_receipt_terms_text,
        "uncollectedAfterDays": shop.uncollected_after_days,
    });
    // Inserted after the fact: the `json!` above is near the macro recursion limit.
    data["shopTradingName"] = json!(shop.trading_name);
    data["shopLegalName"] = json!(shop.legal_name);
    data["shopAddressLines"] = json!(shop.address_lines);
    data["shopPrimaryPhone"] = json!(shop.primary_phone);
    data["shopSecondaryPhone"] = json!(shop.secondary_phone);
    data["shopEmail"] = json!(shop.email);
    data["shopWebsite"] = json!(shop.website);
    Ok(data)
}
