use crate::domain::quotations::STATUS_CANCELLED;

/// Cancelling stamps the PDF, so a cancelled quotation must not be served the
/// cached active one (and vice versa) — the status is part of the key.
pub(super) fn cache_key_for(quotation: &crate::domain::quotations::Quotation) -> &'static str {
    if quotation.status == STATUS_CANCELLED {
        "quotation-cancelled"
    } else {
        "quotation"
    }
}
