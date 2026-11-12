use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::billing::{InvoiceStatus, PaymentRecord, RecordPaymentRequest};
use crate::modules::billing::model::PaymentDocument;
use crate::modules::billing::repository;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

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

/// Lists the payments.
pub async fn list_payments(db: &Db, invoice_key: &str) -> AppResult<Vec<PaymentRecord>> {
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

/// Records the payment.
pub async fn record_payment(
    db: &Db,
    invoice_key: &str,
    body: RecordPaymentRequest,
    recorded_by_user_id: String,
    recorded_by_name: String,
) -> AppResult<PaymentRecord> {
    crate::core::logging::domain::tracked("billing.payment_recorded", async move {
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
        // A payment is only taken against what is owed (total less paid less returned).
        let (_, outstanding) = crate::core::calculations::invoice_paid_and_balance(
            true,
            invoice.total_cents,
            already_paid,
            invoice.refunded_cents,
        );
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

        let resolved_session_doc = crate::modules::sessions::service::resolve_writable_session(
            db,
            body.session_key.as_deref(),
            &recorded_by_user_id,
        )
        .await?;
        let resolved_session_key = resolved_session_doc.as_ref().map(|s| s.key.clone());

        let now = BsonDateTime::now();
        let payment_document = PaymentDocument {
            id: None,
            key: generate_id(prefixes::PAYMENT),
            invoice_key: invoice_key.to_string(),
            amount_cents: body.amount_cents,
            payment_method: body.payment_method.clone(),
            notes: body.notes,
            session_key: resolved_session_key.clone(),
            recorded_by_user_id: recorded_by_user_id.clone(),
            recorded_by_name_snapshot: recorded_by_name,
            recorded_at: now,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        let inserted = repository::insert_payment(db, payment_document).await?;

        if let Some(ref s_key) = resolved_session_key {
            let cash_delta = if body.payment_method == "cash" { inserted.amount_cents } else { 0 };
            let card_delta = if body.payment_method == "card" { inserted.amount_cents } else { 0 };
            let online_delta = if body.payment_method == "online" { inserted.amount_cents } else { 0 };

            crate::modules::sessions::repository::sessions::increment_session_sales(
                db,
                s_key,
                cash_delta,
                card_delta,
                online_delta,
                0,
            )
            .await?;

            if cash_delta != 0 {
                let drawer_key = crate::modules::sessions::service::active_drawer_key(
                    db,
                    resolved_session_doc.as_ref(),
                    &recorded_by_user_id,
                )
                .await?;
                crate::modules::sessions::repository::drawer::update_drawer_balance(
                    db,
                    &drawer_key,
                    cash_delta,
                )
                .await?;
            }
        }

        let new_total_paid = already_paid + inserted.amount_cents;
        let new_status = InvoiceStatus::from_payment_progress(invoice.total_cents, new_total_paid);
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

    })

    .await
}

/// What customers paid and were refunded by method from `since` (`YYYY-MM-DD`) on, as `(method, received, refunded)`.
pub(crate) async fn totals_by_method_since(
    db: &Db,
    since: &str,
) -> AppResult<Vec<(String, i64, i64)>> {
    repository::payment_totals_by_method(db, since).await
}
