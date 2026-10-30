use super::payload::Sticker;
use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::labels::LabelItem;

use crate::modules::inventory;

/// Most distinct products/serials in one print.
const MAX_ITEMS: usize = 200;

/// Most copies of one item, and most stickers in one print (a page each).
const MAX_COPIES: u32 = 500;

const MAX_LABELS_PER_JOB: u64 = 500;

const PRODUCT_WITHOUT_BARCODE: &str = "LABEL_PRODUCT_WITHOUT_BARCODE";

const PRICE_REQUIRED: &str = "LABEL_PRICE_REQUIRED";

fn check_copies(copies: u32) -> AppResult<()> {
    if (1..=MAX_COPIES).contains(&copies) {
        Ok(())
    } else {
        Err(AppError::validation(format!(
            "Copies must be between 1 and {MAX_COPIES}."
        )))
    }
}

/// Turns the requested items into stickers using the stored records. Fails
/// before anything is rendered when an item cannot be printed.
pub(super) async fn resolve_stickers(
    db: &Db,
    needs_price: bool,
    items: &[LabelItem],
) -> AppResult<Vec<Sticker>> {
    if items.is_empty() {
        return Err(AppError::validation("Choose at least one item to print."));
    }
    if items.len() > MAX_ITEMS {
        return Err(AppError::validation(format!(
            "You can print labels for at most {MAX_ITEMS} items at a time."
        )));
    }

    let mut total: u64 = 0;
    let mut stickers = Vec::with_capacity(items.len());
    for item in items {
        let sticker = match item {
            LabelItem::Product {
                product_key,
                copies,
                price_cents,
            } => {
                check_copies(*copies)?;
                let product =
                    inventory::service::product::get_product_by_key(db, product_key).await?;
                let code = product.barcode.clone().ok_or_else(|| {
                    AppError::unprocessable_entity(
                        PRODUCT_WITHOUT_BARCODE,
                        format!("'{}' has no barcode yet. Create one first.", product.name),
                    )
                })?;
                let price = match price_cents {
                    Some(price) => {
                        if !product
                            .price_tiers
                            .iter()
                            .any(|tier| tier.selling_price_cents == *price)
                        {
                            return Err(AppError::unprocessable_entity(
                                codes::PRICE_TIER_UNAVAILABLE,
                                format!("'{}' is not in stock at that price.", product.name),
                            ));
                        }
                        Some(*price)
                    }
                    None if needs_price => {
                        return Err(AppError::unprocessable_entity(
                            PRICE_REQUIRED,
                            format!("Choose a price for '{}'.", product.name),
                        ));
                    }
                    None => None,
                };
                Sticker {
                    product_key: product.key,
                    serial_number: None,
                    name: product.name,
                    sku: product.sku,
                    category: product.category,
                    price_cents: price,
                    code,
                    copies: *copies,
                }
            }
            LabelItem::Serial {
                serial_number,
                copies,
            } => {
                check_copies(*copies)?;
                let lookup =
                    inventory::service::product_serial::lookup_in_stock_serial(db, serial_number)
                        .await?;
                Sticker {
                    product_key: lookup.product.key,
                    serial_number: Some(lookup.serial.serial_number.clone()),
                    name: lookup.product.name,
                    sku: lookup.product.sku,
                    category: lookup.product.category,
                    price_cents: Some(lookup.selling_price_cents),
                    code: lookup.serial.serial_number,
                    copies: *copies,
                }
            }
        };
        total += u64::from(sticker.copies);
        stickers.push(sticker);
    }
    if total > MAX_LABELS_PER_JOB {
        return Err(AppError::validation(format!(
            "You can print at most {MAX_LABELS_PER_JOB} labels at a time ({total} requested)."
        )));
    }
    Ok(stickers)
}
