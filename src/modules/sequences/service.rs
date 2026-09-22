use chrono::{Duration, Utc};

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::sequences::{ReserveSequenceRequest, SequenceReservationResponse},
    modules::sequences::{model::SequenceBlockDocument, repository},
};

pub async fn reserve_sequence(
    db: &Db,
    name: String,
    req: ReserveSequenceRequest,
) -> AppResult<SequenceReservationResponse> {
    crate::core::logging::domain::tracked("sequences.reserved", async move {
        let block_size = req.block_size.unwrap_or(100).clamp(1, 1000) as i64;

        // SKU (`sku:<derived-prefix>`, one family per category+subcategory
        // pair) and barcode reserve from the SAME counters `generate_sku`/
        // `barcode::service::generator::generate` increment one at a time
        // (see those functions), not the generic `sequence_counters` table
        // every other name below uses — so a synced block and a directly
        // created product's SKU/barcode can never collide. Neither one needs
        // the single-number "take from my own block" shortcut here: that
        // shortcut exists for `block_size == 1` calls billing makes at sale
        // time; SKU/barcode's equivalent "take one number now" call happens
        // inside `generate_sku`/`generator::generate` themselves, at product
        // creation, not through this endpoint.
        if let Some(prefix) = name.strip_prefix("sku:") {
            let (prefix, padding, start, end) =
                crate::modules::inventory::service::sku::reserve_sku_block(db, prefix, block_size)
                    .await?;
            return finish_reservation(db, name, prefix, padding, start, end, req.device_id).await;
        }
        if matches!(name.to_lowercase().as_str(), "barcode" | "barcodes") {
            let (start, end) = crate::modules::barcode::service::generator::reserve_block(
                db,
                crate::modules::barcode::service::generator::namespaces::PRODUCT,
                block_size,
            )
            .await?;
            return finish_reservation(db, name, String::new(), 0, start, end, req.device_id).await;
        }

        let (prefix, padding) = match name.to_lowercase().as_str() {
            "invoice" | "invoices" => ("INV-", 6),
            "creditnote" | "creditnotes" | "credit_note" | "credit_notes" => ("CN-", 6),
            "repair" | "repairs" => ("REP-", 6),
            "printjob" | "printjobs" | "print_job" | "print_jobs" => ("PRN-", 6),
            "purchase" | "purchases" => ("PUR-", 6),
            "order" | "orders" => ("ORD-", 6),
            _ => {
                return Err(AppError::validation_with_code(
                    format!("Unknown sequence name: {name}"),
                    codes::SEQUENCE_NOT_FOUND,
                ));
            }
        };

        // A linked desktop device takes single document numbers from the
        // cloud-reserved blocks it holds (or its per-device fallback series),
        // so offline devices never issue the same invoice number.
        if block_size == 1
            && let Some(pool) = db.as_sqlite()
            && let Some(taken) =
                crate::modules::sync::blocks::next_number(pool, &name, prefix, padding).await?
        {
            return Ok(SequenceReservationResponse {
                name,
                block_id: "device-block".to_string(),
                prefix: taken.prefix,
                padding,
                start: taken.number as u64,
                end: taken.number as u64,
                expires_at: Utc::now() + Duration::days(7),
            });
        }

        let (start, end) =
            repository::reserve_block(db, &name, prefix, padding, block_size).await?;

        finish_reservation(db, name, prefix.to_string(), padding, start, end, req.device_id).await
    })
    .await
}

/// Records the reservation in `sequence_blocks` (so `sync::blocks` can track
/// per-device progress the same way for every family) and builds the response.
async fn finish_reservation(
    db: &Db,
    name: String,
    prefix: String,
    padding: usize,
    start: i64,
    end: i64,
    device_id: Option<String>,
) -> AppResult<SequenceReservationResponse> {
    let now = Utc::now();
    let expires_at = now + Duration::days(7);
    let block_id = generate_id(prefixes::SEQUENCE_BLOCK);

    let block_doc = SequenceBlockDocument {
        id: None,
        key: block_id.clone(),
        name: name.clone(),
        start,
        end,
        device_id,
        created_at: mongodb::bson::DateTime::from_chrono(now),
        expires_at: mongodb::bson::DateTime::from_chrono(expires_at),
    };

    repository::insert_block(db, block_doc).await?;

    Ok(SequenceReservationResponse {
        name,
        block_id,
        prefix,
        padding,
        start: start as u64,
        end: end as u64,
        expires_at,
    })
}
