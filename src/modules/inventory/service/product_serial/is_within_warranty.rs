use crate::modules::inventory::model::ProductSerialDocument;

use chrono::Utc;

/// Whether a resolved serial's warranty is still active right now —
/// response-only computation feeding a credit-note line's
/// `within_warranty` field and disposition guidance.
pub(crate) fn is_within_warranty(serial: &ProductSerialDocument) -> Option<bool> {
    serial
        .warranty_expires_at
        .map(|expires| expires.to_chrono() > Utc::now())
}
