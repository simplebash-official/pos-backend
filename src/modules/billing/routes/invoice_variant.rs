use crate::core::error::{AppError, AppResult};

/// The document kind and print-variant cache key of an invoice document:
/// `a4-invoice`, or `thermal-receipt` per paper width (default 80 mm).
pub(super) fn invoice_variant(
    document_type: &str,
    paper_width_mm: Option<u32>,
) -> AppResult<(&'static str, String)> {
    match document_type {
        "a4-invoice" => Ok(("a4-invoice", "a4-invoice".to_string())),
        "thermal-receipt" => {
            let width = paper_width_mm.unwrap_or(80);
            Ok(("thermal-receipt", format!("thermal-receipt-{width}mm")))
        }
        other => Err(AppError::validation(format!(
            "Unknown documentType '{other}'. Must be 'a4-invoice' or 'thermal-receipt'"
        ))),
    }
}
