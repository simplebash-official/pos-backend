// Standalone payment recording — the server-side twin of the frontend's
// already-designed-but-unwired `paymentsStore.ts` (`recordPayment`/
// `deriveInvoiceStatus`). This is how a credit invoice gets paid off over
// time, in one or more installments, independent of `sale::complete_sale`.

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::billing::{InvoiceStatus, PaymentRecord, RecordPaymentRequest},
    modules::billing::{model::PaymentDocument, repository},
};

/// Payments are append-only, same posture as invoices — see
/// `service::hydrate_sync_documents`'s doc comment.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<PaymentRecord>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<PaymentDocument>(document)?.into_payment_record())
        })
        .collect()
}

pub async fn record_payment(
    db: &mongodb::Database,
    invoice_key: &str,
    body: RecordPaymentRequest,
    recorded_by_user_id: String,
    recorded_by_name: String,
) -> AppResult<PaymentRecord> {
    if body.amount_cents <= 0 {
        return Err(AppError::validation("Payment amount must be positive"));
    }

    let invoice = repository::find_invoice_by_key(db, invoice_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
        })?;

    if invoice.status == InvoiceStatus::Voided {
        return Err(AppError::conflict(
            codes::INVOICE_ALREADY_VOIDED,
            "Cannot record a payment against a voided invoice",
        ));
    }

    let existing_payments = repository::list_payments_for_invoice(db, invoice_key).await?;
    let already_paid: i64 = existing_payments.iter().map(|p| p.amount_cents).sum();
    let outstanding = invoice.total_cents - already_paid;
    if body.amount_cents > outstanding {
        return Err(AppError::conflict_with_details(
            codes::PAYMENT_EXCEEDS_BALANCE,
            format!(
                "This payment ({} cents) exceeds the outstanding balance ({} cents)",
                body.amount_cents, outstanding
            ),
            serde_json::json!({ "outstandingCents": outstanding }),
        ));
    }

    let now = BsonDateTime::now();
    let payment_document = PaymentDocument {
        id: None,
        key: generate_id(prefixes::PAYMENT),
        invoice_key: invoice_key.to_string(),
        amount_cents: body.amount_cents,
        payment_method: body.payment_method,
        notes: body.notes,
        recorded_by_user_id,
        recorded_by_name_snapshot: recorded_by_name,
        recorded_at: now,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let inserted = repository::insert_payment(db, payment_document).await?;

    let new_total_paid = already_paid + inserted.amount_cents;
    let new_status = if new_total_paid >= invoice.total_cents {
        InvoiceStatus::Paid
    } else if new_total_paid > 0 {
        InvoiceStatus::PartiallyPaid
    } else {
        InvoiceStatus::Pending
    };
    if new_status != invoice.status
        && let Some(invoice_id) = invoice.id
        && let Err(err) = repository::update_invoice_status(
            db,
            invoice_id,
            doc! { "status": new_status.as_str(), "updated_at": BsonDateTime::now() },
        )
        .await
    {
        tracing::error!(invoice_key, new_status = new_status.as_str(), error = %err, "payment recorded but invoice status update failed");
    }

    if let Some(customer_key) = &invoice.customer_key
        && let Err(err) = crate::modules::customers::service::apply_financial_delta(
            db,
            customer_key,
            0,
            -inserted.amount_cents,
        )
        .await
    {
        tracing::error!(invoice_key, customer_key, error = %err, "payment recorded but customer balance update failed");
    }

    Ok(inserted.into_payment_record())
}

pub async fn list_payments(
    db: &mongodb::Database,
    invoice_key: &str,
) -> AppResult<Vec<PaymentRecord>> {
    // 404s if the invoice itself doesn't exist, so a typo'd key is
    // distinguishable from a real invoice with no payments yet — same
    // convention as `inventory::service::stock::product_movements`.
    repository::find_invoice_by_key(db, invoice_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
        })?;

    let documents = repository::list_payments_for_invoice(db, invoice_key).await?;
    Ok(documents
        .into_iter()
        .map(|d| d.into_payment_record())
        .collect())
}
