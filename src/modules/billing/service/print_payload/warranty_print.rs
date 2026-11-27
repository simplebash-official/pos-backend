//! What printed documents say about warranties: the note under each covered invoice line
//! (A4 invoices and receipts) and the payload of the "Warranty Card" template. Only builds
//! data; reading the warranties and rendering are the callers' job.

use super::service_print_payload::{formatted_date_time, shop_address_lines, shop_field_or_empty};
use crate::core::error::{AppError, AppResult};
use crate::domain::billing::Invoice;
use crate::domain::warranties::{
    Warranty, WarrantyCardPaper, WarrantySourceType, WarrantyStatus, coverage_label,
};
use chrono::NaiveDate;
use serde_json::{Value, json};

/// A stored `YYYY-MM-DD` day as printed (`02 Nov 2027`).
fn printed_day(day: &str) -> AppResult<String> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map(|date| date.format("%d %b %Y").to_string())
        .map_err(|err| AppError::internal(format!("stored warranty day `{day}` is invalid: {err}")))
}

/// The period of a stored warranty as printed. A stored warranty always has a period.
fn printed_coverage(warranty: &Warranty) -> AppResult<String> {
    coverage_label(warranty.coverage).ok_or_else(|| {
        AppError::internal(format!(
            "warranty {} is stored with no coverage",
            warranty.warranty_number
        ))
    })
}

/// The last covered day as printed, or JSON `null` for a lifetime warranty.
fn printed_end(warranty: &Warranty) -> AppResult<Value> {
    match &warranty.ends_on {
        Some(day) => Ok(json!(printed_day(day)?)),
        None => Ok(Value::Null),
    }
}

/// One warranty as the note under its invoice line. `subject` tells several warranties of one
/// line apart: the unit's serial for a sold serial, the work for a repair, empty otherwise.
fn line_note(warranty: &Warranty) -> AppResult<Value> {
    let subject = match warranty.source_type {
        WarrantySourceType::Repair => warranty.item_name.clone(),
        WarrantySourceType::ProductSale => warranty.serial_number.clone().unwrap_or_default(),
    };
    Ok(json!({
        "number": warranty.warranty_number,
        "subject": subject,
        "coverageLabel": printed_coverage(warranty)?,
        "validUntil": printed_end(warranty)?,
        "terms": warranty.terms.clone().unwrap_or_default(),
    }))
}

/// Puts each warranty under its line as `items[i].warranties` (left out on a line without
/// one), for the A4 invoice and the receipt. `warranties` are the invoice's warranties that are
/// not void, in line order.
///
/// # Errors
///
/// An internal error when a warranty points at a line the invoice does not have or holds a
/// value that cannot be printed.
pub(crate) fn add_line_warranties(data: &mut Value, warranties: &[Warranty]) -> AppResult<()> {
    let items = data
        .get_mut("items")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| AppError::internal("print payload has no items list"))?;
    for warranty in warranties {
        let line = usize::try_from(warranty.line_index)
            .ok()
            .and_then(|index| items.get_mut(index))
            .ok_or_else(|| {
                AppError::internal(format!(
                    "warranty {} names line {}, which its invoice does not have",
                    warranty.warranty_number, warranty.line_index
                ))
            })?;
        let note = line_note(warranty)?;
        match line.get_mut("warranties").and_then(Value::as_array_mut) {
            Some(notes) => notes.push(note),
            None => line["warranties"] = json!([note]),
        }
    }
    Ok(())
}

/// Builds `data` for the "Warranty Card" template: one warranty with its shop, customer and
/// invoice. `replaces_number` is the warranty a replacement unit took over from.
///
/// # Errors
///
/// An internal error when the stored warranty holds a value that cannot be printed.
pub(crate) fn build_warranty_card_data(
    warranty: &Warranty,
    invoice: &Invoice,
    paper: WarrantyCardPaper,
    replaces_number: Option<&str>,
) -> AppResult<Value> {
    let shop = &invoice.shop_profile_snapshot;
    let (purchase_date, _) = formatted_date_time(invoice);
    let general_policy = match &invoice.warranty_terms_snapshot {
        Some(text) => text.as_str(),
        None => shop_field_or_empty(shop, "defaultWarrantyText"),
    };
    let mut data = json!({
        "paper": paper.as_str(),
        "logoUrl": shop_field_or_empty(shop, "logoBase64"),
        "warrantyNumber": warranty.warranty_number,
        "sourceType": warranty.source_type.as_str(),
        "itemName": warranty.item_name,
        "serialNumber": warranty.serial_number.clone().unwrap_or_default(),
        "quantity": warranty.quantity,
        "coverageLabel": printed_coverage(warranty)?,
        "warrantyTypeLabel": warranty.warranty_type.label(),
        "startsOn": printed_day(&warranty.starts_on)?,
        "validUntil": printed_end(warranty)?,
        "isVoid": warranty.status == WarrantyStatus::Void,
        "replacesWarrantyNumber": replaces_number.unwrap_or_default(),
        "terms": warranty.terms.clone().unwrap_or_default(),
        "generalPolicy": general_policy,
        "invoiceNumber": invoice.invoice_number,
        "purchaseDate": purchase_date,
        "customerName": warranty.customer_name.clone().unwrap_or_default(),
        "customerPhone": warranty.customer_phone.clone().unwrap_or_default(),
        "shopTradingName": shop_field_or_empty(shop, "tradingName"),
        "shopLegalName": shop_field_or_empty(shop, "legalName"),
        "shopAddressLines": shop_address_lines(shop),
        "shopPrimaryPhone": shop_field_or_empty(shop, "primaryPhone"),
    });
    // Inserted after the fact: the `json!` above is near the macro recursion limit.
    data["shopSecondaryPhone"] = json!(shop_field_or_empty(shop, "secondaryPhone"));
    data["shopEmail"] = json!(shop_field_or_empty(shop, "email"));
    data["shopWebsite"] = json!(shop_field_or_empty(shop, "website"));
    Ok(data)
}
