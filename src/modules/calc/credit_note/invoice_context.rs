use crate::domain::billing::Invoice;

/// What the preview needs from the database, read by the route.
#[derive(Debug)]
pub struct InvoiceContext<'a> {
    pub invoice: &'a Invoice,
    /// What the invoice has been paid so far, one entry per payment (method, cents).
    pub payments: &'a [(String, i64)],
}
