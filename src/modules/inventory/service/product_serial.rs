// Business rules for individually-serialized inventory units: minting
// serials on a purchase receipt, resolving/consuming one at sale time, and
// transitioning a unit's status when it comes back on a credit note. Called
// into from `purchases`/`billing` (sibling top-level modules) via
// `pub(crate)` functions here, the same "reach the other module through its
// service, never its repository" rule every other cross-module dependency
// in this codebase follows.

use chrono::{Duration, Utc};
use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
};

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::inventory::{ProductSerial, SerialStatus},
    modules::inventory::{model::ProductSerialDocument, repository},
};

/// Lists a product's serial units, optionally filtered by lifecycle status.
/// Backs `GET /products/{key}/serials`, used both by a sale-time "pick a
/// unit to sell" picker (`status=in_stock`) and a return-time lookup.
pub(crate) async fn list_serials_for_product(
    db: &Database,
    product_key: &str,
    status: Option<SerialStatus>,
) -> AppResult<Vec<ProductSerial>> {
    let status_str = status.map(|s| {
        serde_json::to_value(s)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string()
    });
    let documents = repository::product_serial::find_serials_for_product(
        db,
        product_key,
        status_str.as_deref(),
    )
    .await?;
    Ok(documents
        .into_iter()
        .map(ProductSerialDocument::into_product_serial)
        .collect())
}

/// Mints one `InStock` serial record per number in `serial_numbers` for a
/// purchase receipt of a serialized product. Rejects (409
/// `SERIAL_ALREADY_EXISTS`) if any given number is already tracked anywhere
/// — serial numbers are unique across the whole shop, not just per product.
pub(crate) async fn create_serials_for_purchase(
    db: &Database,
    product_key: &str,
    serial_numbers: &[String],
) -> AppResult<()> {
    for serial_number in serial_numbers {
        if repository::product_serial::find_serial_by_number(db, serial_number)
            .await?
            .is_some()
        {
            return Err(AppError::custom(
                axum::http::StatusCode::CONFLICT,
                codes::SERIAL_ALREADY_EXISTS,
                format!("Serial number '{serial_number}' is already tracked"),
            ));
        }
    }

    let now = BsonDateTime::now();
    for serial_number in serial_numbers {
        let document = ProductSerialDocument {
            id: None,
            key: generate_id(prefixes::PRODUCT_SERIAL),
            product_key: product_key.to_string(),
            serial_number: serial_number.clone(),
            status: SerialStatus::InStock,
            invoice_key: None,
            sold_at: None,
            warranty_months: None,
            warranty_expires_at: None,
            credit_note_key: None,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        repository::product_serial::insert_product_serial(db, document).await?;
    }
    Ok(())
}

/// Resolves a serial number + product to its Mongo `ObjectId`, regardless of
/// current lifecycle status — used by a credit-note return, which needs to
/// transition a unit that's currently `Sold` (not `InStock`).
pub(crate) async fn find_serial_id(
    db: &Database,
    product_key: &str,
    serial_number: &str,
) -> AppResult<ObjectId> {
    let serial = repository::product_serial::find_serial_by_number_and_product(
        db,
        serial_number,
        product_key,
    )
    .await?
    .ok_or_else(|| {
        AppError::not_found_with_code(
            format!("Serial number '{serial_number}' not found for this product"),
            codes::SERIAL_NOT_FOUND,
        )
    })?;
    serial
        .id
        .ok_or_else(|| AppError::internal("persisted product serial document must have an id"))
}

/// Resolves one serial number to an `InStock` unit of the given product for
/// a sale line — 404 `SERIAL_NOT_FOUND` if unknown/wrong product, 409
/// `SERIAL_ALREADY_SOLD` if it exists but isn't `InStock`.
pub(crate) async fn resolve_in_stock_serial(
    db: &Database,
    product_key: &str,
    serial_number: &str,
) -> AppResult<ProductSerialDocument> {
    let serial = repository::product_serial::find_serial_by_number_and_product(
        db,
        serial_number,
        product_key,
    )
    .await?
    .ok_or_else(|| {
        AppError::not_found_with_code(
            format!("Serial number '{serial_number}' not found for this product"),
            codes::SERIAL_NOT_FOUND,
        )
    })?;

    if serial.status != SerialStatus::InStock {
        return Err(AppError::custom(
            axum::http::StatusCode::CONFLICT,
            codes::SERIAL_ALREADY_SOLD,
            format!("Serial number '{serial_number}' is not available to sell"),
        ));
    }
    Ok(serial)
}

/// Flips a resolved serial to `Sold`, stamping the invoice it was sold on
/// and computing `warranty_expires_at` from the product's snapshotted
/// `warranty_months` (if any). Called post-commit, mirroring how
/// `apply_stock_delta` runs after the invoice itself is already inserted.
pub(crate) async fn mark_serial_sold(
    db: &Database,
    serial_id: ObjectId,
    invoice_key: &str,
    warranty_months: Option<i64>,
) -> AppResult<()> {
    let now = Utc::now();
    let warranty_expires_at =
        warranty_months.map(|months| BsonDateTime::from_chrono(now + Duration::days(months * 30)));

    let mut set_doc = doc! {
        "status": "sold",
        "invoice_key": invoice_key,
        "sold_at": BsonDateTime::from_chrono(now),
    };
    if let Some(months) = warranty_months {
        set_doc.insert("warranty_months", months);
    }
    if let Some(expires) = warranty_expires_at {
        set_doc.insert("warranty_expires_at", expires);
    }

    repository::product_serial::update_serial(db, serial_id, set_doc).await?;
    Ok(())
}

/// Resolves a serial number to the specific `Sold` unit that was sold on
/// `invoice_key` — used when a credit-note line references a serialized
/// invoice line, to confirm the returned unit really was part of that sale.
pub(crate) async fn resolve_sold_serial_for_invoice(
    db: &Database,
    product_key: &str,
    serial_number: &str,
    invoice_key: &str,
) -> AppResult<ProductSerialDocument> {
    let serial = repository::product_serial::find_serial_by_number_and_product(
        db,
        serial_number,
        product_key,
    )
    .await?
    .ok_or_else(|| {
        AppError::not_found_with_code(
            format!("Serial number '{serial_number}' not found for this product"),
            codes::SERIAL_NOT_FOUND,
        )
    })?;

    if serial.status != SerialStatus::Sold || serial.invoice_key.as_deref() != Some(invoice_key) {
        return Err(AppError::not_found_with_code(
            format!("Serial number '{serial_number}' was not sold on this invoice"),
            codes::SERIAL_NOT_FOUND,
        ));
    }
    Ok(serial)
}

/// Whether a resolved serial's warranty is still active right now —
/// response-only computation feeding a credit-note line's
/// `within_warranty` field and disposition guidance.
pub(crate) fn is_within_warranty(serial: &ProductSerialDocument) -> Option<bool> {
    serial
        .warranty_expires_at
        .map(|expires| expires.to_chrono() > Utc::now())
}

/// Transitions a returned serial's status on credit-note creation, per the
/// plan's condition/disposition -> `SerialStatus` mapping:
/// `Resalable`/`OpenBoxDiscount` -> `ReturnedResalable`;
/// `Damaged`+`ReturnToSupplier` -> `UnderWarrantyClaim`;
/// `Damaged`+`WriteOffScrap` -> `WrittenOff`;
/// `Damaged`+`RepairPending` or `PendingInspection` -> `ReturnedFaulty`.
pub(crate) async fn transition_serial_on_return(
    db: &Database,
    serial_id: ObjectId,
    new_status: SerialStatus,
    credit_note_key: &str,
) -> AppResult<()> {
    let status_str = serde_json::to_value(new_status)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let set_doc = doc! {
        "status": status_str,
        "credit_note_key": credit_note_key,
    };
    repository::product_serial::update_serial(db, serial_id, set_doc).await?;
    Ok(())
}

/// Converts a page of raw `product_serials` documents — as read by the sync
/// module's cursor scan — into the `ProductSerial` shape the REST read
/// returns. Read-only mirror: the server is the sole writer of this
/// resource, mirroring `stock_movements`'s sync treatment.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<ProductSerial>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<ProductSerialDocument>(document)?
                    .into_product_serial(),
            )
        })
        .collect()
}
