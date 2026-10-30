use super::close_invoice;
use super::complete_sale;
use super::create_credit_note;
use super::get_credit_note;
use super::get_credit_note_document;
use super::get_credit_note_document_design;
use super::get_invoice;
use super::get_invoice_document;
use super::get_invoice_document_design;
use super::get_invoice_stats;
use super::list_credit_notes;
use super::list_invoices;
use super::list_payments;
use super::record_payment;
use super::status;
use super::void_credit_note;
use super::void_invoice;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(status::status))
        .routes(routes!(complete_sale::complete_sale))
        .routes(routes!(list_invoices::list_invoices))
        .routes(routes!(get_invoice_stats::get_invoice_stats))
        .routes(routes!(get_invoice::get_invoice))
        .routes(routes!(void_invoice::void_invoice))
        .routes(routes!(close_invoice::close_invoice))
        .routes(routes!(get_invoice_document::get_invoice_document))
        .routes(routes!(
            get_invoice_document_design::get_invoice_document_design
        ))
        .routes(routes!(
            record_payment::record_payment,
            list_payments::list_payments
        ))
        .routes(routes!(
            create_credit_note::create_credit_note,
            list_credit_notes::list_credit_notes
        ))
        .routes(routes!(get_credit_note::get_credit_note))
        .routes(routes!(void_credit_note::void_credit_note))
        .routes(routes!(get_credit_note_document::get_credit_note_document))
        .routes(routes!(
            get_credit_note_document_design::get_credit_note_document_design
        ))
}
