//! What voiding an invoice does to the replacements its claims gave out. The void itself puts
//! each line's original units back in stock; for every replacement the customer hands back the
//! unit they have now instead, so that unit returns to its own batch and its serial to stock,
//! while the faulty unit the shop took back stays out of stock.

use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::inventory::{BatchAllocation, StockMovementType};
use crate::modules::inventory;
use crate::modules::warranties::model::WarrantyClaimDocument;
use crate::modules::warranties::repository::{self, ClaimFilter};

/// Reverses every replacement made on the invoice, oldest first, after the void has put the
/// invoice lines' own units back. Each one moves its new unit back in (and releases its
/// serial) and takes the faulty unit it replaced back out, so a chain of replacements nets to
/// the last unit given.
pub(crate) async fn undo_replacements_for_invoice(
    db: &Db,
    invoice_key: &str,
    invoice_number: &str,
) -> AppResult<()> {
    let mut claims = repository::all_claims(
        db,
        &ClaimFilter {
            invoice_key: Some(invoice_key),
            ..ClaimFilter::default()
        },
    )
    .await?;
    claims.retain(|claim| claim.replacement_warranty_key.is_some());
    claims.sort_by_key(|claim| claim.created_at);

    for (index, claim) in claims.iter().enumerate() {
        let product_key = claim
            .product_key
            .as_deref()
            .ok_or_else(|| AppError::internal("a replaced item was sold from stock"))?;
        let given = inventory::service::stock_batch::moved_allocations(
            db,
            product_key,
            StockMovementType::WarrantyReplacement,
            &claim.key,
        )
        .await?;
        let faulty = faulty_unit(db, &claims[..index], claim, product_key, invoice_key).await?;
        let note = format!(
            "Void of invoice {invoice_number}: replacement under {}",
            claim.claim_number
        );
        for (allocations, sign) in [(&given, 1), (&faulty, -1)] {
            inventory::service::stock_batch::move_allocations(
                db,
                product_key,
                allocations,
                sign,
                StockMovementType::InvoiceVoidReversal,
                invoice_key,
                &note,
            )
            .await?;
        }
        if let Some(serial) = &claim.replacement_serial_number {
            inventory::service::product_serial::release_sold_serial(
                db,
                product_key,
                serial,
                invoice_key,
            )
            .await?;
        }
    }
    Ok(())
}

/// The batch of the unit a replacement took back: a serial names its own; otherwise it is the
/// unit an earlier replacement gave out on that warranty, or one of the invoice line's units
/// (the newest batch the sale drew on, as a return picks).
async fn faulty_unit(
    db: &Db,
    earlier: &[WarrantyClaimDocument],
    claim: &WarrantyClaimDocument,
    product_key: &str,
    invoice_key: &str,
) -> AppResult<Vec<BatchAllocation>> {
    if let Some(serial) = &claim.serial_number {
        let batch_key =
            inventory::service::product_serial::serial_batch_key(db, product_key, serial).await?;
        let (_, allocations) = inventory::service::stock_batch::allocations_for_serials(
            db,
            product_key,
            &claim.item_name,
            &[batch_key],
        )
        .await?;
        return Ok(allocations);
    }
    let previous = earlier
        .iter()
        .find(|other| other.replacement_warranty_key.as_deref() == Some(&claim.warranty_key));
    let (movement_type, reference_id) = match previous {
        Some(previous) => (
            StockMovementType::WarrantyReplacement,
            previous.key.as_str(),
        ),
        None => (StockMovementType::Sale, invoice_key),
    };
    let moved = inventory::service::stock_batch::moved_allocations(
        db,
        product_key,
        movement_type,
        reference_id,
    )
    .await?;
    let unit = inventory::service::stock_batch::allocations_for_return(&moved, 0, 1);
    if unit.is_empty() {
        return Err(AppError::internal(format!(
            "no stock movement shows where the unit replaced under {} came from",
            claim.claim_number
        )));
    }
    Ok(unit)
}
