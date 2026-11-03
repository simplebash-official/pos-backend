use super::warranty_issue::phone_tail;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::core::utils::shop_day;
use crate::domain::warranties::{
    Warranty, WarrantyDetail, WarrantyEvent, WarrantyGroup, WarrantyInvoiceLink, WarrantyListQuery,
    WarrantyListResponse, WarrantyLookupMatch, WarrantyLookupResponse,
};
use crate::modules::billing;
use crate::modules::warranties::model::{WarrantyDocument, WarrantyEventDocument};
use crate::modules::warranties::repository::{self, WarrantyFilter};
use chrono::{NaiveDate, Utc};
use mongodb::bson::Document;

/// Most warranties one lookup returns.
const LOOKUP_MAX: u64 = 200;
/// The shortest text a serial-prefix lookup accepts, so two characters do not list the shop.
const SERIAL_PREFIX_MIN: usize = 4;
const LIST_LIMIT_DEFAULT: u64 = 50;
const LIST_LIMIT_MAX: u64 = 200;

fn today() -> NaiveDate {
    shop_day(Utc::now())
}

fn to_api(documents: Vec<WarrantyDocument>, today: NaiveDate) -> AppResult<Vec<Warranty>> {
    documents
        .into_iter()
        .map(|document| document.into_warranty(today))
        .collect()
}

async fn find(db: &Db, filter: WarrantyFilter<'_>) -> AppResult<Vec<WarrantyDocument>> {
    Ok(repository::list(db, &filter, 0, LOOKUP_MAX).await?.0)
}

/// Whether the text could only be a phone number: digits with the usual separators, and at
/// least nine digits.
fn looks_like_phone(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '+' | '-' | '(' | ')'))
        && text.chars().filter(char::is_ascii_digit).count() >= 9
}

/// Groups warranties by device: every warranty of one serial together, and each
/// non-serialized warranty on its own. Groups are newest first, as are their warranties.
fn group(warranties: Vec<Warranty>) -> Vec<WarrantyGroup> {
    let mut groups: Vec<WarrantyGroup> = Vec::new();
    for warranty in warranties {
        let existing = warranty.serial_number.as_ref().and_then(|serial| {
            groups
                .iter_mut()
                .find(|g| g.serial_number.as_ref() == Some(serial))
        });
        match existing {
            Some(group) => group.warranties.push(warranty),
            None => groups.push(WarrantyGroup {
                serial_number: warranty.serial_number.clone(),
                item_name: warranty.item_name.clone(),
                customer_name: warranty.customer_name.clone(),
                customer_phone: warranty.customer_phone.clone(),
                warranties: vec![warranty],
            }),
        }
    }
    groups
}

/// Finds warranties from whatever was scanned or typed. The server decides what the text is:
/// a warranty number (`WTY-`), an invoice number (`INV-`), an exact serial or IMEI, a phone
/// number (matched on its last nine digits), and last a serial that starts with the text.
pub(crate) async fn lookup(db: &Db, q: &str) -> AppResult<WarrantyLookupResponse> {
    let text = q.trim();
    if text.is_empty() {
        return Err(AppError::validation(
            "Scan or type a serial, invoice, warranty number or phone number.",
        ));
    }
    let upper = text.to_uppercase();
    let mut matched_by = WarrantyLookupMatch::Nothing;
    let mut found = Vec::new();
    if upper.starts_with("WTY-") {
        found = find(db, WarrantyFilter::Number(&upper)).await?;
        matched_by = WarrantyLookupMatch::WarrantyNumber;
    } else if upper.starts_with("INV-") {
        found = find(db, WarrantyFilter::InvoiceNumber(&upper)).await?;
        matched_by = WarrantyLookupMatch::InvoiceNumber;
    } else {
        let exact = find(db, WarrantyFilter::Serial(text)).await?;
        if !exact.is_empty() {
            found = exact;
            matched_by = WarrantyLookupMatch::Serial;
        } else if looks_like_phone(text)
            && let Some(tail) = phone_tail(text)
        {
            found = find(db, WarrantyFilter::PhoneTail(&tail)).await?;
            matched_by = WarrantyLookupMatch::Phone;
        }
        if found.is_empty() && text.chars().count() >= SERIAL_PREFIX_MIN {
            found = find(db, WarrantyFilter::SerialPrefix(text)).await?;
            matched_by = WarrantyLookupMatch::Serial;
        }
    }
    if found.is_empty() {
        matched_by = WarrantyLookupMatch::Nothing;
    }
    Ok(WarrantyLookupResponse {
        matched_by,
        groups: group(to_api(found, today())?),
    })
}

/// A page of warranties, newest first, for one customer or one invoice (or all of them).
pub(crate) async fn list(db: &Db, query: WarrantyListQuery) -> AppResult<WarrantyListResponse> {
    let filter = match (&query.customer_key, &query.invoice_key) {
        (Some(_), Some(_)) => {
            return Err(AppError::validation(
                "Filter by a customer or by an invoice, not both.",
            ));
        }
        (Some(customer), None) => WarrantyFilter::Customer(customer),
        (None, Some(invoice)) => WarrantyFilter::InvoiceKey(invoice),
        (None, None) => WarrantyFilter::All,
    };
    let page = query.page.unwrap_or(1).max(1);
    let limit = query
        .limit
        .unwrap_or(LIST_LIMIT_DEFAULT)
        .clamp(1, LIST_LIMIT_MAX);
    let (items, total) = repository::list(db, &filter, (page - 1) * limit, limit).await?;
    Ok(WarrantyListResponse {
        items: to_api(items, today())?,
        total,
        page,
        limit,
    })
}

/// The stored warranty with this key, or 404 `WARRANTY_NOT_FOUND`.
pub(crate) async fn require(db: &Db, key: &str) -> AppResult<WarrantyDocument> {
    repository::find_by_key(db, key).await?.ok_or_else(|| {
        AppError::not_found_with_code("Warranty not found", codes::WARRANTY_NOT_FOUND)
    })
}

async fn linked(db: &Db, key: Option<&str>, today: NaiveDate) -> AppResult<Option<Warranty>> {
    match key {
        None => Ok(None),
        Some(key) => Ok(Some(require(db, key).await?.into_warranty(today)?)),
    }
}

/// One warranty with its activity log, its invoice and its replacement chain.
pub(crate) async fn detail(db: &Db, key: &str) -> AppResult<WarrantyDetail> {
    let today = today();
    let document = require(db, key).await?;
    let events = repository::list_events(db, &document.key)
        .await?
        .into_iter()
        .map(WarrantyEventDocument::into_event)
        .collect::<AppResult<Vec<WarrantyEvent>>>()?;
    let invoice = billing::service::get_invoice(db, &document.invoice_key).await?;
    let replaces = linked(db, document.replaces_warranty_key.as_deref(), today).await?;
    let replaced_by = linked(db, document.replaced_by_warranty_key.as_deref(), today).await?;
    let claims = super::claim_steps::claims_for_warranty(db, &document.key).await?;
    Ok(WarrantyDetail {
        warranty: document.into_warranty(today)?,
        events,
        invoice: WarrantyInvoiceLink {
            key: invoice.key,
            invoice_number: invoice.invoice_number,
            status: invoice.status.as_str().to_string(),
            total_cents: invoice.total_cents,
            created_at: invoice.created_at,
        },
        replaces,
        replaced_by,
        claims,
    })
}

/// Turns raw `warranties` documents read by the sync scan into the REST shape.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<Warranty>> {
    let today = today();
    documents
        .into_iter()
        .map(|document| {
            mongodb::bson::deserialize_from_document::<WarrantyDocument>(document)?
                .into_warranty(today)
        })
        .collect()
}

/// Turns raw `warranty_events` documents read by the sync scan into the REST shape.
pub(crate) fn hydrate_sync_events(documents: Vec<Document>) -> AppResult<Vec<WarrantyEvent>> {
    documents
        .into_iter()
        .map(|document| {
            mongodb::bson::deserialize_from_document::<WarrantyEventDocument>(document)?
                .into_event()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::looks_like_phone;

    #[test]
    fn only_phone_shaped_text_is_read_as_a_phone() {
        assert!(looks_like_phone("0771234567"));
        assert!(looks_like_phone("+94 77 123-4567"));
        assert!(!looks_like_phone("35761234567890A"));
        assert!(!looks_like_phone("12345"));
    }
}
