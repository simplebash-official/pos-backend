// Builds the JSON payload handed to document-server's Typst templates (the
// "A4 Invoice" / "Thermal Receipt" / "Credit Note" templates, addressed by
// the slugs `a4-invoice` / `thermal-receipt` / `credit-note` — see
// `clients::document_server`) from a persisted `Invoice`. Rust port of the
// frontend's `buildPrintPayload.ts` — same field contract, called by
// `modules::documents::service::get_or_render` via `modules::billing::routes`'s
// document-fetch handlers; this module only builds the data, `documents::service`
// renders it.

mod build_a4_invoice_data;
mod build_credit_note_data;
mod build_quotation_data;
mod build_thermal_receipt_data;
mod build_z_report_data;
mod formatted_date_time;
mod number_to_words_rupees;
mod paid_and_balance;
#[cfg(test)]
mod schema_drift_tests;
mod shop_address_lines;
mod shop_bool;
mod shop_field;
mod shop_field_or_empty;
#[cfg(test)]
mod tests;

pub(crate) use build_a4_invoice_data::build_a4_invoice_data;
pub(crate) use build_credit_note_data::build_credit_note_data;
pub(crate) use build_quotation_data::build_quotation_data;
pub(crate) use build_thermal_receipt_data::build_thermal_receipt_data;
pub(crate) use build_z_report_data::build_z_report_data;
