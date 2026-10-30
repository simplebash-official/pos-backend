/// The column that names a row to a person (invoice number, product name…).
/// `None` for rows that have no readable name of their own (payments, stock
/// movements…). Fixed strings only: the result is spliced into SQL.
#[must_use]
pub fn label_column(table: &str) -> Option<&'static str> {
    match table {
        "invoices" => Some("invoice_number"),
        "credit_notes" => Some("credit_note_number"),
        "quotations" => Some("quotation_number"),
        "repairs" | "print_jobs" => Some("ticket_number"),
        "purchases" => Some("receipt_key"),
        "purchase_receipts" => Some("receipt_number"),
        "products" | "customers" | "suppliers" | "employees" | "categories" | "users" => {
            Some("name")
        }
        "product_serials" => Some("serial_number"),
        "register_sessions" => Some("session_number"),
        "cash_movements" => Some("movement_number"),
        "cash_categories" | "payees" => Some("name"),
        "expenses" => Some("expense_number"),
        "receipt_settlements" => Some("payment_key"),
        "supplier_credits" => Some("key"),
        "receipt_charges" => Some("key"),
        _ => None,
    }
}
