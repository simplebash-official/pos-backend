use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::purchase_receipts::{
    PaymentKind, PurchaseReceipt, ReceiptCharge, ReceiptSettlement,
};
use crate::domain::purchases::SupplierSummary;

use crate::modules::purchases::model::{PurchaseDocument, PurchaseReceiptDocument, ReceiptFigures};
use crate::modules::purchases::repository;
use crate::modules::purchases::service::credit as credit_service;
use crate::modules::purchases::service::purchase::enrich_purchases;

use crate::modules::sessions::service::movement as movement_service;
use crate::modules::suppliers::service as supplier_service;

use std::collections::HashMap;

/// Builds the API receipts from their documents and lines: one batch lookup each for the
/// suppliers and (through `enrich_purchases`) the products.
pub(super) async fn assemble(
    db: &Db,
    receipts: Vec<(PurchaseReceiptDocument, Vec<PurchaseDocument>)>,
) -> AppResult<Vec<PurchaseReceipt>> {
    let supplier_keys: Vec<String> = receipts
        .iter()
        .map(|(receipt, _)| receipt.supplier_key.clone())
        .collect();
    let suppliers_by_key: HashMap<String, SupplierSummary> =
        supplier_service::get_suppliers_by_keys(db, &supplier_keys)
            .await?
            .into_iter()
            .map(|supplier| {
                (
                    supplier.key.clone(),
                    SupplierSummary {
                        id: supplier.id,
                        key: supplier.key,
                        name: supplier.name,
                        contact_person: supplier.contact_person,
                        primary_phone: supplier.primary_phone,
                    },
                )
            })
            .collect();

    // What was paid against each receipt. A payment that was later reversed is kept in the list
    // but no longer counts as paid.
    let receipt_keys: Vec<String> = receipts.iter().map(|(r, _)| r.key.clone()).collect();
    let settlements =
        repository::settlement::list_settlements_for_receipts(db, &receipt_keys).await?;
    let cash_payment_keys: Vec<String> = settlements
        .iter()
        .filter(|s| s.payment_kind == PaymentKind::CashMovement)
        .map(|s| s.payment_key.clone())
        .collect();
    let reversed = movement_service::reversed_keys(db, &cash_payment_keys).await?;
    let mut settlements_by_receipt: HashMap<String, Vec<ReceiptSettlement>> = HashMap::new();
    for settlement in settlements {
        let is_reversed = settlement.payment_kind == PaymentKind::CashMovement
            && reversed.contains(&settlement.payment_key);
        settlements_by_receipt
            .entry(settlement.receipt_key.clone())
            .or_default()
            .push(settlement.into_settlement(is_reversed));
    }

    let credited = credit_service::credited_by_receipt(db, &receipt_keys).await?;

    let mut charges_by_receipt: HashMap<String, Vec<ReceiptCharge>> = HashMap::new();
    for charge in repository::charge::list_charges_for_receipts(db, &receipt_keys).await? {
        charges_by_receipt
            .entry(charge.receipt_key.clone())
            .or_default()
            .push(charge.into_charge());
    }

    let mut assembled = Vec::with_capacity(receipts.len());
    for (receipt, lines) in receipts {
        let lines = enrich_purchases(db, lines).await?;
        let supplier = suppliers_by_key.get(&receipt.supplier_key).cloned();
        let figures = ReceiptFigures {
            credited_cents: credited.get(&receipt.key).copied().unwrap_or(0),
            settlements: settlements_by_receipt
                .remove(&receipt.key)
                .unwrap_or_default(),
            charges: charges_by_receipt.remove(&receipt.key).unwrap_or_default(),
        };
        assembled.push(receipt.into_receipt(supplier, lines, figures));
    }
    Ok(assembled)
}
