/// Sequence names arrive as aliases (`invoices`, `credit_note`...); blocks are
/// stored and looked up under one canonical name.
pub(crate) fn canonical_name(name: &str) -> String {
    match name.to_lowercase().replace('_', "").as_str() {
        "invoice" | "invoices" => "invoice",
        "creditnote" | "creditnotes" => "creditnote",
        "repair" | "repairs" => "repair",
        "printjob" | "printjobs" => "printjob",
        "purchase" | "purchases" => "purchase",
        "order" | "orders" => "order",
        "quotation" | "quotations" => "quotation",
        "session" | "sessions" | "registersession" | "registersessions" => "session",
        "cashmovement" | "cashmovements" => "cash_movement",
        "expense" | "expenses" => "expense",
        "income" | "incomes" => "income",
        other => return other.to_string(),
    }
    .to_string()
}
