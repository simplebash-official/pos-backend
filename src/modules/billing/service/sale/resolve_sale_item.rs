use crate::clients::db::Db;
use crate::core::calculations;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::billing::{CreateSaleItemRequest, InvoiceItem};
use crate::domain::inventory::Product;

use crate::modules::{inventory, print_jobs, repairs};

use std::collections::HashMap;

pub(crate) async fn resolve_sale_item(
    db: &Db,
    item: &CreateSaleItemRequest,
    products: &HashMap<String, Product>,
) -> AppResult<InvoiceItem> {
    let (
        name,
        sku,
        unit_price_cents,
        source_ticket_number,
        assigned_employee_name,
        unit_cost_cents,
        batch_allocations,
    ) = match item.source_type.as_str() {
        "retail" => match &item.product_key {
            Some(product_key) => {
                let product = products.get(product_key).ok_or_else(|| {
                    AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
                })?;

                // Every unit comes out of a stock batch, and the line is priced
                // at the batches it draws on: a serialized line at its chosen
                // units' batch, any other line at the tier the cashier picked
                // (consumed oldest batch first). Planned here, before any
                // write; applied in `apply_line_item_side_effects`.
                let (unit_price_cents, allocations) = if product.is_serialized {
                    let serial_numbers = item.serial_numbers.clone().unwrap_or_default();
                    if serial_numbers.len() as i64 != item.quantity {
                        return Err(AppError::validation(format!(
                            "'{}' is serialized — provide exactly {} serial number(s), got {}",
                            product.name,
                            item.quantity,
                            serial_numbers.len()
                        )));
                    }
                    // Fail before any write if a serial is unknown or
                    // already sold (D4's "resolve everything first" rule) —
                    // the actual `Sold` transition happens post-commit in
                    // `apply_line_item_side_effects`, same as the stock
                    // decrement it accompanies.
                    let mut batch_keys = Vec::with_capacity(serial_numbers.len());
                    for serial_number in &serial_numbers {
                        let serial = inventory::service::product_serial::resolve_in_stock_serial(
                            db,
                            &product.key,
                            serial_number,
                        )
                        .await?;
                        batch_keys.push(serial.batch_key);
                    }
                    inventory::service::stock_batch::allocations_for_serials(
                        db,
                        &product.key,
                        &product.name,
                        &batch_keys,
                    )
                    .await?
                } else {
                    let price = item.selling_price_cents.ok_or_else(|| {
                        AppError::validation(format!(
                            "Pick the price '{}' is sold at — sellingPriceCents is required",
                            product.name
                        ))
                    })?;
                    let allocations = inventory::service::stock_batch::plan_tier_allocation(
                        db,
                        &product.key,
                        &product.name,
                        price,
                        item.quantity,
                    )
                    .await?;
                    (price, allocations)
                };

                (
                    product.name.clone(),
                    Some(product.sku.clone()),
                    unit_price_cents,
                    None,
                    None,
                    Some(inventory::service::stock_batch::weighted_unit_cost(
                        &allocations,
                    )),
                    allocations,
                )
            }
            None => (
                item.name.clone().ok_or_else(|| {
                    AppError::validation("An item with no productKey must include a name")
                })?,
                None,
                item.unit_price_cents.ok_or_else(|| {
                    AppError::validation("An item with no productKey must include unitPriceCents")
                })?,
                None,
                None,
                None,
                Vec::new(),
            ),
        },
        "repair" => match &item.source_ticket_key {
            Some(ticket_key) => {
                let ticket = repairs::service::get_repair(db, ticket_key).await?;
                let estimated_cost_cents = ticket.estimated_cost_cents.ok_or_else(|| {
                    AppError::validation_with_code(
                        format!(
                            "Repair ticket {} has no price yet, set a repair price before billing it",
                            ticket.ticket_number
                        ),
                        codes::REPAIR_PRICE_REQUIRED,
                    )
                })?;
                (
                    item.name
                        .clone()
                        .unwrap_or_else(|| ticket.device_model.clone()),
                    None,
                    estimated_cost_cents + ticket.material_cost_cents.unwrap_or(0),
                    Some(ticket.ticket_number),
                    ticket.assigned_employee_name,
                    Some(ticket.material_cost_cents.unwrap_or(0) / item.quantity.max(1)),
                    Vec::new(),
                )
            }
            None => (
                item.name.clone().ok_or_else(|| {
                    AppError::validation(
                        "A repair item with no sourceTicketKey must include a name",
                    )
                })?,
                None,
                item.unit_price_cents.ok_or_else(|| {
                    AppError::validation(
                        "A repair item with no sourceTicketKey must include unitPriceCents",
                    )
                })?,
                None,
                item.assigned_employee_name.clone(),
                None,
                Vec::new(),
            ),
        },
        "print" => match &item.source_ticket_key {
            Some(ticket_key) => {
                let ticket = print_jobs::service::get_print_job(db, ticket_key).await?;
                (
                    item.name.clone().unwrap_or_else(|| ticket.job_type.clone()),
                    None,
                    ticket.estimated_cost_cents + ticket.material_cost_cents.unwrap_or(0),
                    Some(ticket.ticket_number),
                    ticket.assigned_employee_name,
                    Some(ticket.material_cost_cents.unwrap_or(0) / item.quantity.max(1)),
                    Vec::new(),
                )
            }
            None => (
                item.name.clone().ok_or_else(|| {
                    AppError::validation("A print item with no sourceTicketKey must include a name")
                })?,
                None,
                item.unit_price_cents.ok_or_else(|| {
                    AppError::validation(
                        "A print item with no sourceTicketKey must include unitPriceCents",
                    )
                })?,
                None,
                item.assigned_employee_name.clone(),
                None,
                Vec::new(),
            ),
        },
        other => {
            return Err(AppError::validation(format!(
                "Invalid item sourceType '{other}'"
            )));
        }
    };

    let total_cents =
        calculations::compute_line_total(unit_price_cents, item.quantity, item.discount_cents);

    Ok(InvoiceItem {
        product_key: item.product_key.clone(),
        name,
        sku,
        unit_price_cents,
        quantity: item.quantity,
        discount_cents: item.discount_cents,
        total_cents,
        unit_cost_cents,
        source_type: item.source_type.clone(),
        source_ticket_key: item.source_ticket_key.clone(),
        source_ticket_number,
        assigned_employee_name,
        returned_quantity: 0,
        serial_numbers: item.serial_numbers.clone().unwrap_or_default(),
        batch_allocations,
    })
}
