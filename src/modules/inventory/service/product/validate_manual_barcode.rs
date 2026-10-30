use crate::core::error::{AppError, AppResult};

/// Loose validation for a staff-entered manual barcode: numeric digits
/// only, 8-14 characters — spans common real-world formats (EAN-8/UPC-A/
/// EAN-13/GTIN-14) a scanned product might already carry. Deliberately does
/// not require the EAN-13 checksum to be valid — that check only applies to
/// codes this system generates itself (see `barcode::service::ean13`).
pub(super) fn validate_manual_barcode(value: &str) -> AppResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || !trimmed.chars().all(|c| c.is_ascii_digit())
        || !(8..=14).contains(&trimmed.len())
    {
        return Err(AppError::validation("Barcode must be 8-14 numeric digits"));
    }
    Ok(())
}
