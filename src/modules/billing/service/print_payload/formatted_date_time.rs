use crate::domain::billing::Invoice;

pub(super) fn formatted_date_time(invoice: &Invoice) -> (String, String) {
    let formatted_date = invoice.created_at.format("%d %b %Y").to_string();
    let formatted_time = invoice.created_at.format("%H:%M").to_string();
    (formatted_date, formatted_time)
}
