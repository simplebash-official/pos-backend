use super::warranty_events::{self, NewEvent, WarrantyActor};
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::utils::shop_day;
use crate::domain::repairs::RepairItem;
use crate::domain::sequences::ReserveSequenceRequest;
use crate::domain::warranties::{
    ProductWarranty, WarrantyEventKind, WarrantySourceType, WarrantyStatus,
};
use crate::modules::sequences;
use crate::modules::warranties::model::WarrantyDocument;
use crate::modules::warranties::repository::{self, WarrantyFilter};
use chrono::{DateTime, Utc};
use mongodb::bson::DateTime as BsonDateTime;
use pos_calc_core::warranty::{self, WarrantyEnd};

/// Most warranties one invoice can carry: one per sold unit of a long invoice.
const INVOICE_WARRANTIES_MAX: u64 = 1000;

/// One sold invoice line, as the warranty needs it.
#[derive(Debug)]
pub(crate) struct SaleLine<'a> {
    pub invoice_key: &'a str,
    pub invoice_number: &'a str,
    pub sold_at: DateTime<Utc>,
    pub customer_key: Option<&'a str>,
    pub customer_name: Option<&'a str>,
    pub customer_phone: Option<&'a str>,
    /// Zero-based position of the line on the invoice.
    pub line_index: usize,
    pub product_key: &'a str,
    pub item_name: &'a str,
    pub quantity: i64,
    pub unit_price_cents: i64,
    /// One warranty each when the product is serialized; empty otherwise.
    pub serial_numbers: &'a [String],
    /// The product's warranty, or the cashier's override for this line.
    pub warranty: &'a ProductWarranty,
    pub actor: &'a WarrantyActor,
    pub device_id: Option<String>,
}

/// The last nine digits of a phone number, which is what a phone lookup compares, so
/// `0771234567` and `+94 77 123 4567` are the same customer. `None` with fewer than nine.
#[must_use]
pub(crate) fn phone_tail(phone: &str) -> Option<String> {
    let digits: Vec<char> = phone.chars().filter(char::is_ascii_digit).collect();
    (digits.len() >= 9).then(|| digits[digits.len() - 9..].iter().collect())
}

/// The next number of a sequence (`warranty` gives `WTY-000001`, `warranty_claim` `WCL-…`).
pub(super) async fn next_number(
    db: &Db,
    sequence: &str,
    device_id: Option<String>,
) -> AppResult<String> {
    let reservation = sequences::service::reserve_sequence(
        db,
        sequence.to_string(),
        ReserveSequenceRequest {
            block_size: Some(1),
            device_id,
        },
    )
    .await?;
    Ok(format!(
        "{}{:0width$}",
        reservation.prefix,
        reservation.start,
        width = reservation.padding
    ))
}

/// Gives a sold line its warranties: one per serial of a serialized product, one for the whole
/// line otherwise, none when the line is not covered. Each starts on the shop-local day of the
/// sale and keeps a copy of the coverage, type and terms, so a later product edit never changes
/// a warranty already given.
pub(crate) async fn issue_for_sale_line(
    db: &Db,
    line: SaleLine<'_>,
) -> AppResult<Vec<WarrantyDocument>> {
    if !line.warranty.is_covered() {
        return Ok(Vec::new());
    }
    let warranty_type = line.warranty.warranty_type.ok_or_else(|| {
        AppError::internal("a covered product warranty always has a warranty type")
    })?;
    let starts_on = shop_day(line.sold_at);
    let ends_on = match warranty::ends_on(starts_on, line.warranty.coverage)? {
        WarrantyEnd::On(day) => Some(day.to_string()),
        WarrantyEnd::Never => None,
    };
    let units: Vec<(Option<&String>, i64)> = if line.serial_numbers.is_empty() {
        vec![(None, line.quantity)]
    } else {
        line.serial_numbers.iter().map(|s| (Some(s), 1)).collect()
    };
    let line_index = i64::try_from(line.line_index).expect("an invoice has few lines");

    let mut issued = Vec::with_capacity(units.len());
    for (serial_number, quantity) in units {
        let now = BsonDateTime::now();
        let document = WarrantyDocument {
            id: None,
            key: generate_id(prefixes::WARRANTY),
            warranty_number: next_number(db, "warranty", line.device_id.clone()).await?,
            source_type: WarrantySourceType::ProductSale,
            invoice_key: line.invoice_key.to_string(),
            invoice_number: line.invoice_number.to_string(),
            line_index,
            product_key: Some(line.product_key.to_string()),
            repair_key: None,
            repair_item_key: None,
            item_name: line.item_name.to_string(),
            serial_number: serial_number.cloned(),
            quantity,
            customer_key: line.customer_key.map(str::to_string),
            customer_name: line.customer_name.map(str::to_string),
            customer_phone: line.customer_phone.map(str::to_string),
            customer_phone_tail: line.customer_phone.and_then(phone_tail),
            coverage: line.warranty.coverage,
            warranty_type,
            terms: line.warranty.terms.clone(),
            unit_price_cents: line.unit_price_cents,
            starts_on: starts_on.to_string(),
            ends_on: ends_on.clone(),
            status: WarrantyStatus::Active,
            void_reason: None,
            replaces_warranty_key: None,
            replaced_by_warranty_key: None,
            claim_count: 0,
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        };
        let inserted = repository::insert_warranty(db, document).await?;
        warranty_events::record(
            db,
            NewEvent {
                warranty_key: &inserted.key,
                claim_key: None,
                kind: WarrantyEventKind::Created,
                from_status: None,
                to_status: Some(WarrantyStatus::Active.as_str()),
                note: Some(format!("Sold on {}", line.invoice_number)),
                actor: line.actor,
                approved_by: None,
            },
        )
        .await?;
        issued.push(inserted);
    }
    Ok(issued)
}

/// A handed-back repair job, as its items' warranties need it.
#[derive(Debug)]
pub(crate) struct RepairLine<'a> {
    pub invoice_key: &'a str,
    pub invoice_number: &'a str,
    pub delivered_at: DateTime<Utc>,
    pub customer_key: Option<&'a str>,
    pub customer_name: Option<&'a str>,
    pub customer_phone: Option<&'a str>,
    /// Zero-based position of the job's line on the invoice.
    pub line_index: usize,
    pub repair_key: &'a str,
    pub ticket_number: &'a str,
    pub device_model: &'a str,
    /// The customer's device, which is what staff scan when it comes back.
    pub device_serial: Option<&'a str>,
    pub items: &'a [RepairItem],
    pub actor: &'a WarrantyActor,
    pub device_id: Option<String>,
}

/// Gives each covered item of a handed-back repair job its own warranty, starting on the
/// shop-local day it was handed back, against the customer's device serial. An uncovered item
/// gets none. The item's description and device name it, so one job can carry a screen under
/// six months and a cleaning under none.
pub(crate) async fn issue_for_repair(
    db: &Db,
    line: RepairLine<'_>,
) -> AppResult<Vec<WarrantyDocument>> {
    let starts_on = shop_day(line.delivered_at);
    let line_index = i64::try_from(line.line_index).expect("an invoice has few lines");
    let mut issued = Vec::new();
    for item in line.items {
        if !item.warranty.is_covered() {
            continue;
        }
        let warranty_type = item.warranty.warranty_type.ok_or_else(|| {
            AppError::internal("a covered repair item warranty always has a warranty type")
        })?;
        let ends_on = match warranty::ends_on(starts_on, item.warranty.coverage)? {
            WarrantyEnd::On(day) => Some(day.to_string()),
            WarrantyEnd::Never => None,
        };
        let now = BsonDateTime::now();
        let document = WarrantyDocument {
            id: None,
            key: generate_id(prefixes::WARRANTY),
            warranty_number: next_number(db, "warranty", line.device_id.clone()).await?,
            source_type: WarrantySourceType::Repair,
            invoice_key: line.invoice_key.to_string(),
            invoice_number: line.invoice_number.to_string(),
            line_index,
            product_key: None,
            repair_key: Some(line.repair_key.to_string()),
            repair_item_key: Some(item.key.clone()),
            item_name: format!("{} ({})", item.description, line.device_model),
            serial_number: line.device_serial.map(str::to_string),
            quantity: 1,
            customer_key: line.customer_key.map(str::to_string),
            customer_name: line.customer_name.map(str::to_string),
            customer_phone: line.customer_phone.map(str::to_string),
            customer_phone_tail: line.customer_phone.and_then(phone_tail),
            coverage: item.warranty.coverage,
            warranty_type,
            terms: item.warranty.terms.clone(),
            unit_price_cents: item.labor_cents + item.part_cents,
            starts_on: starts_on.to_string(),
            ends_on,
            status: WarrantyStatus::Active,
            void_reason: None,
            replaces_warranty_key: None,
            replaced_by_warranty_key: None,
            claim_count: 0,
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        };
        let inserted = repository::insert_warranty(db, document).await?;
        warranty_events::record(
            db,
            NewEvent {
                warranty_key: &inserted.key,
                claim_key: None,
                kind: WarrantyEventKind::Created,
                from_status: None,
                to_status: Some(WarrantyStatus::Active.as_str()),
                note: Some(format!(
                    "Repair {} handed back on {}",
                    line.ticket_number, line.invoice_number
                )),
                actor: line.actor,
                approved_by: None,
            },
        )
        .await?;
        issued.push(inserted);
    }
    Ok(issued)
}

/// Cancels one warranty (unless it already is) and logs why. Rereads and tries again, up to
/// three times, when another write changed it in between.
async fn void_one(
    db: &Db,
    mut warranty: WarrantyDocument,
    reason: &str,
    actor: &WarrantyActor,
) -> AppResult<()> {
    for _ in 0..3 {
        if warranty.status == WarrantyStatus::Void {
            return Ok(());
        }
        let from = warranty.status;
        let expected = warranty.version;
        warranty.status = WarrantyStatus::Void;
        warranty.void_reason = Some(reason.to_string());
        warranty.version += 1;
        warranty.updated_at = BsonDateTime::now();
        if repository::replace_warranty(db, &warranty, expected).await? {
            return warranty_events::record(
                db,
                NewEvent {
                    warranty_key: &warranty.key,
                    claim_key: None,
                    kind: WarrantyEventKind::Voided,
                    from_status: Some(from.as_str()),
                    to_status: Some(WarrantyStatus::Void.as_str()),
                    note: Some(reason.to_string()),
                    actor,
                    approved_by: None,
                },
            )
            .await;
        }
        warranty = repository::find_by_key(db, &warranty.key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Warranty not found", codes::WARRANTY_NOT_FOUND)
            })?;
    }
    Err(AppError::conflict(
        codes::VERSION_CONFLICT,
        "The warranty kept changing while it was being cancelled. Try again.",
    ))
}

async fn invoice_warranties(db: &Db, invoice_key: &str) -> AppResult<Vec<WarrantyDocument>> {
    let (items, _) = repository::list(
        db,
        &WarrantyFilter::InvoiceKey(invoice_key),
        0,
        INVOICE_WARRANTIES_MAX,
    )
    .await?;
    Ok(items)
}

/// Cancels every warranty of a voided invoice.
pub(crate) async fn void_for_invoice(
    db: &Db,
    invoice_key: &str,
    invoice_number: &str,
    actor: &WarrantyActor,
) -> AppResult<()> {
    let reason = format!("Invoice {invoice_number} was voided");
    for warranty in invoice_warranties(db, invoice_key).await? {
        void_one(db, warranty, &reason, actor).await?;
    }
    Ok(())
}

/// Cancels the warranty of a serialized unit returned on a credit note.
pub(crate) async fn void_for_returned_serial(
    db: &Db,
    invoice_key: &str,
    serial_number: &str,
    credit_note_number: &str,
    actor: &WarrantyActor,
) -> AppResult<()> {
    let reason = format!("Returned on {credit_note_number}");
    for warranty in invoice_warranties(db, invoice_key).await? {
        if warranty.serial_number.as_deref() == Some(serial_number) {
            void_one(db, warranty, &reason, actor).await?;
        }
    }
    Ok(())
}

/// Cancels the warranty of a non-serialized line once all of it has come back.
pub(crate) async fn void_for_returned_line(
    db: &Db,
    invoice_key: &str,
    line_index: usize,
    credit_note_number: &str,
    actor: &WarrantyActor,
) -> AppResult<()> {
    let reason = format!("Returned on {credit_note_number}");
    let line_index = i64::try_from(line_index).expect("an invoice has few lines");
    for warranty in invoice_warranties(db, invoice_key).await? {
        if warranty.line_index == line_index && warranty.serial_number.is_none() {
            void_one(db, warranty, &reason, actor).await?;
        }
    }
    Ok(())
}

/// Whether the unit sold on `invoice_key` with this serial is still covered today: `None` when
/// it never had a warranty.
pub(crate) async fn serial_within_warranty(
    db: &Db,
    invoice_key: &str,
    serial_number: &str,
) -> AppResult<Option<bool>> {
    let today = shop_day(Utc::now());
    for warranty in invoice_warranties(db, invoice_key).await? {
        if warranty.serial_number.as_deref() != Some(serial_number) {
            continue;
        }
        if warranty.status == WarrantyStatus::Void {
            return Ok(Some(false));
        }
        let ends_on = warranty
            .ends_on
            .as_deref()
            .map(|text| crate::modules::warranties::model::stored_day("ends_on", text))
            .transpose()?;
        return Ok(Some(
            warranty::clock(ends_on, today) != warranty::WarrantyClock::Expired,
        ));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::phone_tail;

    #[test]
    fn phone_numbers_match_on_their_last_nine_digits() {
        assert_eq!(phone_tail("0771234567").as_deref(), Some("771234567"));
        assert_eq!(phone_tail("+94 77 123 4567").as_deref(), Some("771234567"));
        assert_eq!(phone_tail("077-123-4567").as_deref(), Some("771234567"));
        assert_eq!(phone_tail("12345"), None);
    }
}
