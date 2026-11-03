use crate::clients::db::Db;
use crate::core::calculations;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::billing::{
    CreateSaleItemRequest, Invoice, InvoiceItem, InvoiceStatus, VoidInvoiceRequest,
};
use crate::domain::inventory::{Product, StockMovementType};
use crate::modules::billing::repository;
use crate::modules::{customers, inventory, print_jobs, repairs, warranties};
use mongodb::bson::{DateTime as BsonDateTime, doc};
use std::collections::HashMap;

/// Marks a fully paid invoice `Closed` — a manual, explicit "done, nothing
/// more will happen to this invoice" action (never auto-computed). Blocked
/// unless the invoice is `Paid` and has no open (non-voided) credit notes
/// against it; re-queries `credit_notes` live for that guard rather than
/// trusting `Invoice.credit_note_count` alone.
pub async fn close_invoice(db: &Db, id_or_key: &str, closed_by: String) -> AppResult<Invoice> {
    crate::core::logging::domain::tracked("billing.invoice_closed", async move {
        let existing = repository::find_invoice_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
            })?;

        if existing.status != InvoiceStatus::Paid {
            return Err(AppError::conflict(
                codes::INVOICE_NOT_CLOSABLE,
                "Only a fully paid invoice can be closed",
            ));
        }

        let open_credit_notes =
            repository::credit_notes::count_open_credit_notes_for_invoice(db, &existing.key)
                .await?;
        if open_credit_notes > 0 {
            return Err(AppError::conflict(
                codes::INVOICE_NOT_CLOSABLE,
                "This invoice has an open credit note — void or resolve it before closing",
            ));
        }

        let object_id = existing
            .id
            .expect("persisted invoice document must have an _id");
        let set_doc = doc! {
            "status": InvoiceStatus::Closed.as_str(),
            "closed_at": BsonDateTime::now(),
            "closed_by": &closed_by,
            "updated_at": BsonDateTime::now(),
        };

        let updated = repository::update_invoice_status(db, object_id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
            })?;

        Ok(updated.into_invoice())
    })
    .await
}

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
                if ticket.kind.is_rework() {
                    return Err(AppError::validation_with_code(
                        format!(
                            "Repair ticket {} is a free warranty rework; hand it back from the repair screen instead of billing it",
                            ticket.ticket_number
                        ),
                        codes::REPAIR_IS_REWORK,
                    ));
                }
                if ticket.status == "delivered" {
                    return Err(AppError::conflict(
                        codes::INVALID_JOB_STATUS,
                        format!(
                            "Repair ticket {} has already been handed back and billed",
                            ticket.ticket_number
                        ),
                    ));
                }
                if ticket.items.is_empty() {
                    return Err(AppError::validation_with_code(
                        format!(
                            "Repair ticket {} has no work on it yet, add the work done before billing it",
                            ticket.ticket_number
                        ),
                        codes::REPAIR_PRICE_REQUIRED,
                    ));
                }
                // A part sold in the meantime stops the sale before anything is written.
                repairs::service::check_parts_in_stock(db, &ticket).await?;
                (
                    item.name
                        .clone()
                        .unwrap_or_else(|| ticket.device_model.clone()),
                    None,
                    ticket.price_cents,
                    Some(ticket.ticket_number),
                    ticket.assigned_employee_name,
                    Some(ticket.cost_cents / item.quantity.max(1)),
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

/// Resolves each request-side item into a persisted-shape `InvoiceItem`,
/// pulling `name`/`sku`/`unitPriceCents` (and, for repair/print, the
/// assigned employee) from the referenced product/ticket rather than
/// trusting the client — mirrors the customer-key resolution just above it
/// in `complete_sale`. Also returns the resolved `Product`s keyed by
/// `product_key` so the post-commit stock-decrement loop doesn't have to
/// look them up a second time. An item whose `productKey`/`sourceTicketKey`
/// doesn't resolve fails the whole request (via `?`) before any write,
/// exactly like a bad `customerKey` already does.
///
/// Retail products are fetched in one batched `$in` query up front (see
/// `inventory::service::product::get_products_by_keys`) rather than one
/// `get_product_by_key` round trip per item — a large cart previously paid 3
/// sequential Mongo round trips per retail line just for this lookup.
/// Remaining per-item work (repair/print ticket lookups) runs concurrently
/// via `try_join_all` instead of a sequential loop, so the whole resolution
/// step costs roughly one round trip's worth of latency regardless of cart
/// size, not N.
pub(crate) async fn resolve_sale_items(
    db: &Db,
    items: &[CreateSaleItemRequest],
) -> AppResult<(Vec<InvoiceItem>, HashMap<String, Product>)> {
    // Each line's batches are planned against current stock independently,
    // so two lines at the same product+price would both plan on the same
    // units. The cart keeps one line per price; refuse anything else.
    let mut seen_tiers = std::collections::HashSet::new();
    for item in items {
        if let (Some(product_key), Some(price)) = (&item.product_key, item.selling_price_cents)
            && !seen_tiers.insert((product_key.as_str(), price))
        {
            return Err(AppError::validation(
                "The same product at the same price appears on two lines — combine them into one",
            ));
        }
    }
    let retail_product_keys: Vec<String> = items
        .iter()
        .filter(|item| item.source_type == "retail")
        .filter_map(|item| item.product_key.clone())
        .collect();
    let products: HashMap<String, Product> =
        inventory::service::product::get_products_by_keys(db, &retail_product_keys)
            .await?
            .into_iter()
            .map(|product| (product.key.clone(), product))
            .collect();

    let resolved: Vec<InvoiceItem> = futures_util::future::try_join_all(
        items
            .iter()
            .map(|item| resolve_sale_item(db, item, &products)),
    )
    .await?;

    Ok((resolved, products))
}

/// Void (D8): reverses retail stock and customer financials, sets
/// `status: Voided` with a mandatory reason and audit trail. This is the
/// single "invalidated after the fact" action — see
/// `InvoiceStatus::Voided`'s doc comment for why there is no separate
/// zero-impact "Cancelled" status in this codebase. Does **not** touch
/// repair/print-job ticket status (ambiguous what "un-delivering" a ticket
/// should mean — flagged as a warning for manual follow-up instead) and
/// refuses to run at all if any payment beyond the original sale-time one
/// has been recorded (reversing partial credit repayments is out of scope
/// for this version).
pub async fn void_invoice(
    db: &Db,
    id_or_key: &str,
    body: VoidInvoiceRequest,
    voided_by: String,
    voided_by_name: String,
) -> AppResult<(Invoice, Vec<String>)> {
    crate::core::logging::domain::tracked("billing.invoice_voided", async move {
        if body.reason.trim().is_empty() {
            return Err(AppError::validation(
                "A reason is required to void an invoice",
            ));
        }

        let existing = repository::find_invoice_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
            })?;

        if existing.status == InvoiceStatus::Voided {
            return Err(AppError::conflict(
                codes::INVOICE_ALREADY_VOIDED,
                "This invoice has already been voided",
            ));
        }

        // How many payment rows `complete_sale` itself would have inserted —
        // for a credit sale, one if a deposit was taken at checkout else zero;
        // one per split leg for a split payment; otherwise exactly one. Any
        // payment beyond that count means a repayment (or credit-note refund)
        // has since been recorded, which this basic void flow cannot safely
        // reverse.
        let expected_payment_count: usize = if existing.is_credit {
            usize::from(existing.amount_received_cents.unwrap_or(0) > 0)
        } else if existing.payment_method == "split" {
            existing
                .split_payments
                .as_ref()
                .map_or(1, |legs| legs.len())
        } else {
            1
        };
        let payment_count = repository::count_payments_for_invoice(db, &existing.key).await?;
        if payment_count as usize > expected_payment_count {
            return Err(AppError::conflict(
                codes::INVOICE_HAS_PAYMENTS_CANNOT_VOID,
                "This invoice has payments recorded beyond the original sale and cannot be voided automatically",
            ));
        }

        let object_id = existing
            .id
            .expect("persisted invoice document must have an _id");
        let mut warnings: Vec<String> = Vec::new();

        for item in &existing.items {
            if item.source_type != "retail" {
                if matches!(item.source_type.as_str(), "repair" | "print") {
                    warnings.push(format!(
                        "Invoice voided — ticket for '{}' was not reverted and may need manual review",
                        item.name
                    ));
                }
                continue;
            }
            let Some(product_key) = item.product_key.as_deref() else {
                continue;
            };
            if let Err(err) = inventory::service::stock_batch::move_allocations(
                db,
                product_key,
                &item.batch_allocations,
                1,
                StockMovementType::InvoiceVoidReversal,
                &existing.key,
                &format!("Void of invoice {}", existing.invoice_number),
            )
            .await
            {
                warnings.push(format!(
                    "Stock for '{}' could not be restored: {err}",
                    item.name
                ));
            }
            for serial_number in &item.serial_numbers {
                if let Err(err) = inventory::service::product_serial::release_sold_serial(
                    db,
                    product_key,
                    serial_number,
                    &existing.key,
                )
                .await
                {
                    warnings.push(format!(
                        "Serial {serial_number} of '{}' could not be put back in stock: {err}",
                        item.name
                    ));
                }
            }
        }

        let actor = warranties::service::WarrantyActor {
            user_id: voided_by.clone(),
            name: voided_by_name,
        };
        if let Err(err) = warranties::service::void_for_invoice(
            db,
            &existing.key,
            &existing.invoice_number,
            &actor,
        )
        .await
        {
            warnings.push(format!("The invoice's warranties could not be cancelled: {err}"));
        }

        if let Some(customer_key) = &existing.customer_key {
            // Only the amount that actually landed on the customer's account
            // (total minus any checkout deposit) was ever added to their owed
            // balance, so only that much is reversed. The deposit payment row
            // itself is left in place (same as a voided cash sale keeps its
            // sale-time payment row).
            let balance_delta = if existing.is_credit {
                -(existing.total_cents - existing.amount_received_cents.unwrap_or(0))
            } else {
                0
            };
            if let Err(err) = customers::service::apply_financial_delta(
                db,
                customer_key,
                -existing.total_cents,
                balance_delta,
            )
            .await
            {
                warnings.push(format!("Customer financials could not be reversed: {err}"));
            }
        }

        let set_doc = doc! {
            "status": InvoiceStatus::Voided.as_str(),
            "voided_at": BsonDateTime::now(),
            "voided_by": &voided_by,
            "voided_reason": &body.reason,
            "updated_at": BsonDateTime::now(),
        };

        let updated = repository::update_invoice_status(db, object_id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
            })?;

        Ok((updated.into_invoice(), warnings))
    })
    .await
}
