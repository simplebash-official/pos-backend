// Business rules and orchestration for customer returns, refunds (cashback),
// and replacements/exchanges. Gated by `CurrentUser` + `billing:write` permission.

use std::collections::HashMap;

use futures_util::future::try_join_all;
use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
};

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        utils::{build_bson_regex, calculate_pagination, today_utc_range},
    },
    domain::{
        billing::{
            CreateReturnRequest, CreateSaleItemRequest, InvoiceItem, ItemRestockAction, ReturnItem,
            ReturnListQuery, ReturnListResponse, ReturnRecord,
        },
        inventory::{Product, StockMovementType},
        sequences::ReserveSequenceRequest,
    },
    modules::{
        billing::{
            model::{PaymentDocument, ReturnDocument},
            repository,
        },
        customers, inventory, print_jobs, repairs, sequences,
    },
};

/// Resolves exchange sale items from catalog / tickets.
async fn resolve_exchange_items(
    db: &Database,
    items: &[CreateSaleItemRequest],
) -> AppResult<(Vec<InvoiceItem>, HashMap<String, Product>)> {
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

    let resolved: Vec<InvoiceItem> = try_join_all(
        items
            .iter()
            .map(|item| resolve_exchange_item(db, item, &products)),
    )
    .await?;

    Ok((resolved, products))
}

async fn resolve_exchange_item(
    db: &Database,
    item: &CreateSaleItemRequest,
    products: &HashMap<String, Product>,
) -> AppResult<InvoiceItem> {
    let (name, sku, unit_price_cents, source_ticket_number, assigned_employee_name) = match item
        .source_type
        .as_str()
    {
        "retail" => match &item.product_key {
            Some(product_key) => {
                let product = products.get(product_key).ok_or_else(|| {
                    AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
                })?;
                (
                    product.name.clone(),
                    Some(product.sku.clone()),
                    product.selling_price_cents,
                    None,
                    None,
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
            ),
        },
        other => {
            return Err(AppError::validation(format!(
                "Invalid item sourceType '{other}'"
            )));
        }
    };

    let total_cents = (unit_price_cents * item.quantity - item.discount_cents).max(0);

    Ok(InvoiceItem {
        product_key: item.product_key.clone(),
        name,
        sku,
        unit_price_cents,
        quantity: item.quantity,
        discount_cents: item.discount_cents,
        total_cents,
        source_type: item.source_type.clone(),
        source_ticket_key: item.source_ticket_key.clone(),
        source_ticket_number,
        assigned_employee_name,
        returned_quantity: 0,
    })
}

/// Creates a return, processes stock adjustments, calculates cashback or difference,
/// and updates the original invoice.
pub async fn create_return(
    db: &Database,
    body: CreateReturnRequest,
    cashier_id: String,
    cashier_name: String,
    device_id: Option<String>,
) -> AppResult<ReturnRecord> {
    if body.returned_items.is_empty() {
        return Err(AppError::validation("At least one item must be returned"));
    }

    for item_req in &body.returned_items {
        if item_req.quantity < 1 {
            return Err(AppError::validation(
                "Return item quantity must be at least 1",
            ));
        }
    }

    // Find the original invoice
    let invoice = repository::find_invoice_by_id_or_key(db, &body.invoice_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
        })?;

    if invoice.status == "cancelled" {
        return Err(AppError::conflict(
            codes::INVOICE_NOT_RETURNABLE,
            "Cancelled invoice cannot be returned",
        ));
    }

    let mut updated_invoice_items = invoice.items.clone();
    let mut resolved_return_items = Vec::new();
    let mut return_subtotal_cents: i64 = 0;

    for item_req in &body.returned_items {
        let restock_action = match item_req.restock_inventory {
            Some(true) => ItemRestockAction::RestockToInventory,
            Some(false) => ItemRestockAction::DamagedDiscard,
            None => item_req.restock_action,
        };

        // Find matching item on invoice
        let matched_idx = updated_invoice_items
            .iter()
            .position(|item| {
                if let (Some(req_pk), Some(item_pk)) = (&item_req.product_key, &item.product_key)
                    && req_pk == item_pk
                {
                    return true;
                }
                if let (Some(req_stk), Some(item_stk)) =
                    (&item_req.source_ticket_key, &item.source_ticket_key)
                    && req_stk == item_stk
                {
                    return true;
                }
                if let Some(req_name) = &item_req.name
                    && req_name.eq_ignore_ascii_case(&item.name)
                {
                    return true;
                }
                // If the invoice has only one item and the return has only one item, match it
                if updated_invoice_items.len() == 1 && body.returned_items.len() == 1 {
                    return true;
                }
                false
            })
            .ok_or_else(|| {
                AppError::validation_with_code(
                    format!(
                        "Item '{}' was not found on invoice {}",
                        item_req.name.as_deref().unwrap_or("unknown"),
                        invoice.invoice_number
                    ),
                    codes::VALIDATION_ERROR,
                )
            })?;

        let inv_item = &mut updated_invoice_items[matched_idx];
        let available_qty = inv_item.quantity - inv_item.returned_quantity;

        if item_req.quantity > available_qty {
            return Err(AppError::custom(
                axum::http::StatusCode::BAD_REQUEST,
                codes::RETURN_QUANTITY_EXCEEDED,
                format!(
                    "Return quantity ({}) exceeds available returnable quantity ({}) for item '{}'",
                    item_req.quantity, available_qty, inv_item.name
                ),
            ));
        }

        // Calculate unit price and total refunded cents for this line item
        let unit_price_cents = if inv_item.quantity > 0 {
            inv_item.total_cents / inv_item.quantity
        } else {
            inv_item.unit_price_cents
        };
        let line_total_cents = unit_price_cents * item_req.quantity;
        return_subtotal_cents += line_total_cents;

        inv_item.returned_quantity += item_req.quantity;

        resolved_return_items.push(ReturnItem {
            product_key: inv_item.product_key.clone(),
            name: inv_item.name.clone(),
            sku: inv_item.sku.clone(),
            quantity: item_req.quantity,
            unit_price_cents,
            total_cents: line_total_cents,
            reason: item_req.reason,
            restock_action,
            notes: item_req.notes.clone(),
            source_type: Some(inv_item.source_type.clone()),
            source_ticket_key: inv_item.source_ticket_key.clone(),
            source_ticket_number: inv_item.source_ticket_number.clone(),
        });
    }

    // Resolve exchange / replacement items if provided
    let (resolved_exchange_items, resolved_exchange_products) =
        if let Some(exchange_reqs) = &body.exchange_items {
            if exchange_reqs.is_empty() {
                (Vec::new(), HashMap::new())
            } else {
                resolve_exchange_items(db, exchange_reqs).await?
            }
        } else {
            (Vec::new(), HashMap::new())
        };

    let exchange_subtotal_cents: i64 = resolved_exchange_items
        .iter()
        .map(|item| item.total_cents)
        .sum();

    let net_refund_cents = return_subtotal_cents - exchange_subtotal_cents;

    // Reserve sequence number for return
    let reservation = sequences::service::reserve_sequence(
        db,
        "return".to_string(),
        ReserveSequenceRequest {
            block_size: Some(1),
            device_id,
        },
    )
    .await?;

    let return_number = format!(
        "{}{:0width$}",
        reservation.prefix,
        reservation.start,
        width = reservation.padding
    );
    let return_key = generate_id(prefixes::RETURN);

    // Apply stock delta for returned items
    for ret_item in &resolved_return_items {
        if ret_item.restock_action == ItemRestockAction::RestockToInventory
            && let Some(product_key) = &ret_item.product_key
            && let Ok(product) =
                inventory::service::product::get_product_by_key(db, product_key).await
            && let Ok(product_object_id) = ObjectId::parse_str(&product.id)
            && let Err(err) = inventory::service::stock::apply_stock_delta(
                db,
                product_object_id,
                ret_item.quantity,
                StockMovementType::Return,
                Some(return_key.clone()),
                Some(format!("Restock from return {return_number}")),
            )
            .await
        {
            tracing::warn!(product_key, error = %err, "could not restock returned product");
        }
    }

    // Apply stock delta for exchange items (deduct from inventory)
    for ex_item in &resolved_exchange_items {
        if ex_item.source_type == "retail"
            && let Some(product_key) = &ex_item.product_key
            && let Some(product) = resolved_exchange_products.get(product_key)
            && let Ok(product_object_id) = ObjectId::parse_str(&product.id)
            && let Err(err) = inventory::service::stock::apply_stock_delta(
                db,
                product_object_id,
                -ex_item.quantity,
                StockMovementType::Sale,
                Some(return_key.clone()),
                Some(format!("Exchange replacement on return {return_number}")),
            )
            .await
        {
            tracing::warn!(product_key, error = %err, "could not deduct stock for exchange product");
        }
    }

    let now = BsonDateTime::now();
    let payment_method = body.payment_method.unwrap_or_else(|| "cash".to_string());
    let mut refund_payment_key = None;

    // Financial side effects
    if net_refund_cents > 0 {
        // Cashback payout
        let pay_key = generate_id(prefixes::PAYMENT);
        let payment_doc = PaymentDocument {
            id: None,
            key: pay_key.clone(),
            invoice_key: invoice.key.clone(),
            amount_cents: -net_refund_cents,
            payment_method: payment_method.clone(),
            notes: Some(format!("Cashback refund for return {return_number}")),
            recorded_by_user_id: cashier_id.clone(),
            recorded_by_name_snapshot: cashier_name.clone(),
            recorded_at: now,
            version: 1,
            created_at: now,
            updated_at: now,
        };

        if let Ok(inserted_payment) = repository::insert_payment(db, payment_doc).await {
            refund_payment_key = Some(inserted_payment.key);
        }

        if let Some(customer_key) = &invoice.customer_key {
            let balance_delta = if invoice.is_credit {
                -net_refund_cents
            } else {
                0
            };
            if let Err(err) = customers::service::apply_financial_delta(
                db,
                customer_key,
                -net_refund_cents,
                balance_delta,
            )
            .await
            {
                tracing::warn!(customer_key, error = %err, "could not adjust customer balance on return");
            }
        }
    } else if net_refund_cents < 0 {
        // Customer pays extra for exchange
        let extra_cents = -net_refund_cents;
        let pay_key = generate_id(prefixes::PAYMENT);
        let payment_doc = PaymentDocument {
            id: None,
            key: pay_key.clone(),
            invoice_key: invoice.key.clone(),
            amount_cents: extra_cents,
            payment_method: payment_method.clone(),
            notes: Some(format!(
                "Payment for exchange difference on return {return_number}"
            )),
            recorded_by_user_id: cashier_id.clone(),
            recorded_by_name_snapshot: cashier_name.clone(),
            recorded_at: now,
            version: 1,
            created_at: now,
            updated_at: now,
        };

        if let Ok(inserted_payment) = repository::insert_payment(db, payment_doc).await {
            refund_payment_key = Some(inserted_payment.key);
        }

        if let Some(customer_key) = &invoice.customer_key {
            let balance_delta = if invoice.is_credit { extra_cents } else { 0 };
            if let Err(err) = customers::service::apply_financial_delta(
                db,
                customer_key,
                extra_cents,
                balance_delta,
            )
            .await
            {
                tracing::warn!(customer_key, error = %err, "could not adjust customer balance on return exchange");
            }
        }
    }

    // Atomically update invoice items with returned_quantity and refunded_cents
    if let Some(invoice_id) = invoice.id {
        repository::invoice::update_invoice_return_progress(
            db,
            invoice_id,
            updated_invoice_items,
            return_subtotal_cents,
        )
        .await?;
    }

    // Persist return record
    let return_doc = ReturnDocument {
        id: None,
        key: return_key,
        return_number,
        invoice_key: invoice.key,
        invoice_number: invoice.invoice_number,
        customer_key: invoice.customer_key,
        customer_name_snapshot: invoice.customer_name_snapshot,
        cashier_id,
        cashier_name_snapshot: cashier_name,
        returned_items: resolved_return_items,
        exchange_items: resolved_exchange_items,
        return_subtotal_cents,
        exchange_subtotal_cents,
        net_refund_cents,
        payment_method: Some(payment_method),
        refund_payment_key,
        notes: body.notes,
        version: 1,
        created_at: now,
        updated_at: now,
    };

    let inserted = repository::returns::insert_return(db, return_doc).await?;
    Ok(inserted.into_return_record())
}

/// Look up a single return by its hex ObjectId or unique model key (`ret_...`).
pub async fn get_return(db: &Database, id_or_key: &str) -> AppResult<ReturnRecord> {
    let document = repository::returns::find_return_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Return record not found", codes::RETURN_NOT_FOUND)
        })?;
    Ok(document.into_return_record())
}

/// List returns matching query filters with pagination.
pub async fn list_returns(db: &Database, query: ReturnListQuery) -> AppResult<ReturnListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.trim().is_empty()) {
        let pattern = build_bson_regex(search.trim());
        and_clauses.push(doc! {
            "$or": [
                { "return_number": { "$regex": pattern.clone() } },
                { "invoice_number": { "$regex": pattern.clone() } },
                { "customer_name_snapshot": { "$regex": pattern } },
            ]
        });
    }
    if let Some(invoice_key) = query.invoice_key.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "invoice_key": invoice_key.trim() });
    }
    if let Some(customer_key) = query.customer_key.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "customer_key": customer_key.trim() });
    }
    if query.date_preset.as_deref() == Some("today") {
        let (today_start, today_end) = today_utc_range();
        and_clauses.push(doc! {
            "created_at": {
                "$gte": BsonDateTime::from_chrono(today_start),
                "$lt": BsonDateTime::from_chrono(today_end),
            }
        });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 200);
    let (documents, total) = repository::returns::list_returns(db, filter, skip, limit).await?;
    let returns = documents
        .into_iter()
        .map(|d| d.into_return_record())
        .collect();
    let total_pages = total.div_ceil(limit);

    Ok(ReturnListResponse {
        returns,
        total,
        page,
        limit,
        total_pages,
    })
}

/// Converts a page of raw `returns` documents — as read by the sync
/// module's cursor scan — into the `ReturnRecord` shape the REST reads return.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<ReturnRecord>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<ReturnDocument>(document)?.into_return_record())
        })
        .collect()
}
