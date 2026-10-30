use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::billing::CreditNote;

use crate::modules::billing::repository;

/// Look up a single credit note by its hex ObjectId or unique model key (`cn_...`).
pub async fn get_credit_note(db: &Db, id_or_key: &str) -> AppResult<CreditNote> {
    let document = repository::credit_notes::find_credit_note_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Credit note not found", codes::CREDIT_NOTE_NOT_FOUND)
        })?;
    Ok(document.into_credit_note())
}
