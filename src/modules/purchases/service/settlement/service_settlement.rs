use crate::clients::db::Db;
use crate::clients::uow::Tx;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::purchase_receipts::{PaymentKind, ReceiptSettlementRequest};
use crate::modules::purchases::model::ReceiptSettlementDocument;
use crate::modules::purchases::repository;
use crate::modules::purchases::service::receipt::get_receipt;
use mongodb::bson::DateTime as BsonDateTime;
use std::collections::HashSet;

/// Converts raw `receipt_settlements` documents, as the sync module reads them, into the wire
/// shape. `reversed` is worked out when a receipt is read and is dropped by the receiver.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<crate::domain::purchase_receipts::ReceiptSettlement>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<ReceiptSettlementDocument>(document)
                    .map_err(AppError::from)?
                    .into_settlement(false),
            )
        })
        .collect()
}

/// Checks that every named receipt belongs to this supplier and is owed at least what is put
/// against it, and that the parts add up to no more than the payment. Nothing is written.
pub(crate) async fn prepare_settlements(
    db: &Db,
    supplier_key: &str,
    payment_cents: i64,
    requests: &[ReceiptSettlementRequest],
) -> AppResult<PreparedSettlements> {
    let mut seen = HashSet::new();
    let mut total = 0;
    let mut parts = Vec::with_capacity(requests.len());
    for request in requests {
        if request.amount_cents <= 0 {
            return Err(AppError::validation(
                "The amount put against a receipt must be more than zero",
            ));
        }
        if !seen.insert(request.receipt_key.clone()) {
            return Err(AppError::validation("A receipt can be listed only once"));
        }
        let receipt = get_receipt(db, &request.receipt_key).await?;
        if receipt.supplier_key != supplier_key {
            return Err(AppError::validation(
                "That receipt belongs to another supplier",
            ));
        }
        if request.amount_cents > receipt.due_cents {
            return Err(AppError::conflict(
                codes::RECEIPT_OVERPAID,
                format!(
                    "{} is owed less than that.",
                    receipt
                        .reference_no
                        .as_deref()
                        .unwrap_or(&receipt.receipt_number)
                ),
            ));
        }
        total += request.amount_cents;
        parts.push((request.receipt_key.clone(), request.amount_cents));
    }
    if total > payment_cents {
        return Err(AppError::validation(
            "More is put against receipts than is being paid",
        ));
    }
    Ok(PreparedSettlements {
        supplier_key: supplier_key.to_string(),
        parts,
    })
}

/// Settlements checked and ready to write once the payment's key is known.
#[derive(Debug)]
pub(crate) struct PreparedSettlements {
    pub(super) supplier_key: String,
    pub(super) parts: Vec<(String, i64)>,
}

/// Writes the settlements for a payment, in the payment's transaction.
pub(crate) async fn write_settlements(
    tx: &mut Tx,
    prepared: PreparedSettlements,
    payment_kind: PaymentKind,
    payment_key: &str,
) -> AppResult<()> {
    let now = BsonDateTime::now();
    for (receipt_key, amount_cents) in prepared.parts {
        repository::settlement::insert_settlement(
            &mut *tx,
            ReceiptSettlementDocument {
                id: None,
                key: generate_id(prefixes::RECEIPT_SETTLEMENT),
                receipt_key,
                supplier_key: prepared.supplier_key.clone(),
                amount_cents,
                payment_kind,
                payment_key: payment_key.to_string(),
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
    }
    Ok(())
}
