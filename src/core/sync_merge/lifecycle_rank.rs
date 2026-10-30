#![forbid(unsafe_code)]

/// Position of `status` in the monotonic lifecycle of `resource`, or `None`
/// for an unknown resource/status. Higher rank always wins a merge.
#[must_use]
pub fn lifecycle_rank(resource: &str, status: &str) -> Option<u8> {
    match (resource, status) {
        ("invoices" | "invoice", "pending") => Some(0),
        ("invoices" | "invoice", "partially_paid") => Some(1),
        ("invoices" | "invoice", "paid") => Some(2),
        ("invoices" | "invoice", "closed") => Some(3),
        ("invoices" | "invoice", "voided") => Some(4),
        ("creditNotes" | "credit_notes" | "creditNote", "awaiting_resolution") => Some(0),
        ("creditNotes" | "credit_notes" | "creditNote", "resolved") => Some(1),
        ("creditNotes" | "credit_notes" | "creditNote", "voided") => Some(2),
        ("quotations" | "quotation", "active") => Some(0),
        ("quotations" | "quotation", "fulfilled") => Some(1),
        ("quotations" | "quotation", "cancelled") => Some(2),
        _ => None,
    }
}
