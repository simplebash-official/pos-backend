//! Builds the JSON payload handed to document-server's Typst templates (the
//! "A4 Invoice" / "Thermal Receipt" / "Credit Note" templates, addressed by
//! the slugs `a4-invoice` / `thermal-receipt` / `credit-note` — see
//! `clients::document_server`) from a persisted `Invoice`. Rust port of the
//! frontend's `buildPrintPayload.ts` — same field contract, called by
//! `modules::documents::service::get_or_render` via `modules::billing::routes`'s
//! document-fetch handlers; this module only builds the data, `documents::service`
//! renders it.

#[cfg(test)]
mod schema_drift_tests;
mod service_print_payload;
#[cfg(test)]
mod tests;

pub(crate) use service_print_payload::build_a4_invoice_data;
pub(crate) use service_print_payload::build_credit_note_data;
pub(crate) use service_print_payload::build_quotation_data;
pub(crate) use service_print_payload::build_thermal_receipt_data;
pub(crate) use service_print_payload::build_z_report_data;
