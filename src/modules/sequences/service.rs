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

    let block_size = req.block_size.unwrap_or(100).clamp(1, 1000) as i64;
    let (start, end) = repository::reserve_block(db, &name, prefix, padding, block_size).await?;

    let now = Utc::now();
    let expires_at = now + Duration::days(7);
    let block_id = generate_id(prefixes::SEQUENCE_BLOCK);

    let block_doc = SequenceBlockDocument {
        id: None,
        key: block_id.clone(),
        name: name.clone(),
        start,
        end,
        device_id: req.device_id,
        created_at: mongodb::bson::DateTime::from_chrono(now),
        expires_at: mongodb::bson::DateTime::from_chrono(expires_at),
    };

    repository::insert_block(db, block_doc).await?;

    Ok(SequenceReservationResponse {
        name,
        block_id,
        prefix: prefix.to_string(),
        padding,
        start: start as u64,
        end: end as u64,
        expires_at,
    })
}
