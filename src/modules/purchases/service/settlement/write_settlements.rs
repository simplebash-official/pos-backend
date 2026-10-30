use super::prepared_settlements::PreparedSettlements;

use crate::clients::uow::Tx;
use crate::core::constants::prefixes;
use crate::core::error::AppResult;
use crate::core::id::generate_id;
use crate::domain::purchase_receipts::PaymentKind;
use crate::modules::purchases::model::ReceiptSettlementDocument;
use crate::modules::purchases::repository;

use mongodb::bson::DateTime as BsonDateTime;

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
