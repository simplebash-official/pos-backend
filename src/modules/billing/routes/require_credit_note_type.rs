use crate::core::error::{AppError, AppResult};

pub(super) fn require_credit_note_type(document_type: &str) -> AppResult<()> {
    if document_type != "credit-note" {
        return Err(AppError::validation(format!(
            "Unknown documentType '{document_type}'. Must be 'credit-note'"
        )));
    }
    Ok(())
}
