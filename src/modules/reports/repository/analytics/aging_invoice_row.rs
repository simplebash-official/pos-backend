// ---------------------------------------------------------------------------
// receivables aging
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct AgingInvoiceRow {
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub balance_due_cents: i64,
    pub days_overdue: i32,
}
