use super::today::today;
use crate::clients::db::Db;
use crate::core::calculations::{compute_line_total, compute_order_discount};
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;

use crate::domain::billing::PricingAdjustments;
use crate::domain::inventory::Product;
use crate::domain::quotations::{
    CreateQuotationItem, CreateQuotationRequest, Quotation, QuotationItem, STATUS_ACTIVE,
};
use crate::domain::sequences::ReserveSequenceRequest;
use crate::modules::quotations::model::QuotationDocument;
use crate::modules::quotations::repository;

use crate::modules::{customers, inventory, sequences, settings};
use chrono::{NaiveDate, Utc};

use mongodb::bson::DateTime as BsonDateTime;
use std::collections::HashMap;

const MAX_ITEMS: usize = 200;

const MAX_NAME_LEN: usize = 200;

const MAX_TEXT_LEN: usize = 2000;

fn validate_valid_until(valid_until: &str) -> AppResult<()> {
    let date = NaiveDate::parse_from_str(valid_until, "%Y-%m-%d").map_err(|_| {
        AppError::validation("Valid-until date must be a real date written as YYYY-MM-DD")
    })?;
    if date < Utc::now().date_naive() {
        return Err(AppError::validation(
            "Valid-until date cannot be in the past",
        ));
    }
    Ok(())
}

fn trimmed_optional(value: Option<String>, label: &str, max: usize) -> AppResult<Option<String>> {
    let value = value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    if let Some(v) = &value
        && v.chars().count() > max
    {
        return Err(AppError::validation(format!(
            "{label} must be at most {max} characters"
        )));
    }
    Ok(value)
}

fn validate_adjustments(adjustments: Option<&PricingAdjustments>) -> AppResult<()> {
    let Some(adj) = adjustments else {
        return Ok(());
    };
    if adj.discount_type != "percentage" && adj.discount_type != "fixed" {
        return Err(AppError::validation(
            "Discount type must be 'percentage' or 'fixed'",
        ));
    }
    if !adj.discount_value.is_finite() || adj.discount_value < 0.0 {
        return Err(AppError::validation("Discount cannot be negative"));
    }
    if adj.discount_type == "percentage" && adj.discount_value > 100.0 {
        return Err(AppError::validation("Discount cannot be more than 100%"));
    }
    Ok(())
}

/// Turns one requested line into a priced `QuotationItem`. A `productKey`
/// makes the catalog authoritative for name, SKU and price (a client-typed
/// duplicate could silently diverge); without one the line is ad hoc and must
/// carry its own name and price.
async fn resolve_item(db: &Db, line: CreateQuotationItem) -> AppResult<QuotationItem> {
    if line.quantity < 1 {
        return Err(AppError::validation("Quantity must be at least 1"));
    }
    if line.discount_cents < 0 {
        return Err(AppError::validation("Line discount cannot be negative"));
    }

    let (product_key, name, sku, unit_price_cents) = match line.product_key {
        Some(key) => {
            let product = inventory::service::product::get_product_by_key(db, &key).await?;
            let price = line.selling_price_cents.ok_or_else(|| {
                AppError::validation(format!(
                    "Pick the price to quote '{}' at — sellingPriceCents is required",
                    product.name
                ))
            })?;
            if price <= 0 {
                return Err(AppError::validation(format!(
                    "Item '{}' needs a price",
                    product.name
                )));
            }
            // A quotation never touches stock. If the product has in-stock
            // tiers, it must match one of them; if out of stock, any valid
            // positive price is allowed.
            if !product.price_tiers.is_empty()
                && !product
                    .price_tiers
                    .iter()
                    .any(|tier| tier.selling_price_cents == price)
            {
                return Err(AppError::conflict_with_details(
                    codes::PRICE_TIER_UNAVAILABLE,
                    format!("{} is not in stock at this price.", product.name),
                    serde_json::json!({ "sellingPriceCents": price }),
                ));
            }
            (Some(product.key), product.name, Some(product.sku), price)
        }
        None => {
            let name = line.name.map(|n| n.trim().to_string()).unwrap_or_default();
            if name.is_empty() {
                return Err(AppError::validation(
                    "Each item needs a name (or pick a product)",
                ));
            }
            let price = line
                .unit_price_cents
                .ok_or_else(|| AppError::validation(format!("Item '{name}' needs a price")))?;
            (None, name, None, price)
        }
    };

    if name.chars().count() > MAX_NAME_LEN {
        return Err(AppError::validation(format!(
            "Item names must be at most {MAX_NAME_LEN} characters"
        )));
    }
    if unit_price_cents < 0 {
        return Err(AppError::validation(format!(
            "Item '{name}' cannot have a negative price"
        )));
    }
    let gross = unit_price_cents
        .checked_mul(line.quantity)
        .ok_or_else(|| AppError::validation(format!("Item '{name}' total is too large")))?;
    if line.discount_cents > gross {
        return Err(AppError::validation(format!(
            "Discount on '{name}' cannot be more than the item's price"
        )));
    }

    Ok(QuotationItem {
        product_key,
        name,
        sku,
        unit_price_cents,
        quantity: line.quantity,
        discount_cents: line.discount_cents,
        total_cents: compute_line_total(unit_price_cents, line.quantity, line.discount_cents),
    })
}

/// Creates a quotation. Every figure is recomputed here; the shop profile is
/// read from settings and frozen onto the row (never taken from the client).
pub async fn create_quotation(
    db: &Db,
    tenant_id: Option<&str>,
    body: CreateQuotationRequest,
    device_id: Option<String>,
) -> AppResult<Quotation> {
    crate::core::logging::domain::tracked("quotations.created", async move {
        if body.items.is_empty() {
            return Err(AppError::validation("Add at least one item"));
        }
        if body.items.len() > MAX_ITEMS {
            return Err(AppError::validation(format!(
                "A quotation can have at most {MAX_ITEMS} items"
            )));
        }
        validate_valid_until(&body.valid_until)?;
        validate_adjustments(body.pricing_adjustments.as_ref())?;
        let notes = trimmed_optional(body.notes, "Notes", MAX_TEXT_LEN)?;
        let terms = trimmed_optional(body.terms, "Terms", MAX_TEXT_LEN)?;
        let prepared_by = trimmed_optional(body.prepared_by, "Prepared by", MAX_NAME_LEN)?;

        // Resolve everything that can fail before reserving a number, so a
        // rejected request never burns one.
        let (customer_key, customer_name, customer_phone, customer_address) =
            match body.customer.customer_key {
                Some(key) => {
                    let c = customers::service::get_customer_by_key(db, &key).await?;
                    (Some(c.key), c.name, Some(c.primary_phone), c.address)
                }
                None => {
                    let name = body
                        .customer
                        .customer_name
                        .map(|n| n.trim().to_string())
                        .unwrap_or_default();
                    if name.is_empty() {
                        return Err(AppError::validation("Customer name is required"));
                    }
                    (
                        None,
                        name,
                        trimmed_optional(body.customer.customer_phone, "Phone", MAX_NAME_LEN)?,
                        trimmed_optional(body.customer.customer_address, "Address", MAX_TEXT_LEN)?,
                    )
                }
            };

        let mut items = Vec::with_capacity(body.items.len());
        for line in body.items {
            items.push(resolve_item(db, line).await?);
        }

        let mut has_out_of_stock_items = false;
        let product_keys: Vec<String> = items
            .iter()
            .filter_map(|item| item.product_key.clone())
            .collect();
        if !product_keys.is_empty() {
            let products =
                inventory::service::product::get_products_by_keys(db, &product_keys).await?;
            let product_map: HashMap<String, Product> =
                products.into_iter().map(|p| (p.key.clone(), p)).collect();
            for item in &items {
                if let Some(key) = &item.product_key
                    && let Some(p) = product_map.get(key)
                    && p.stock_quantity < item.quantity
                {
                    has_out_of_stock_items = true;
                    break;
                }
            }
        }

        let subtotal_cents: i64 = items.iter().map(|i| i.total_cents).sum();
        let (discount_type, discount_value, discount_cents) =
            compute_order_discount(subtotal_cents, body.pricing_adjustments.as_ref());
        let total_cents = (subtotal_cents - discount_cents).max(0);

        let shop = settings::service::get_shop_profile(db, tenant_id).await?;
        let shop_profile_snapshot = serde_json::to_value(shop)
            .map_err(|e| AppError::internal(format!("Could not read the shop profile: {e}")))?;

        let reservation = sequences::service::reserve_sequence(
            db,
            "quotation".to_string(),
            ReserveSequenceRequest {
                block_size: Some(1),
                device_id: device_id.clone(),
            },
        )
        .await?;
        let quotation_number = format!(
            "{}{:0width$}",
            reservation.prefix,
            reservation.start,
            width = reservation.padding
        );

        let now = BsonDateTime::now();
        let inserted = repository::insert(
            db,
            QuotationDocument {
                id: None,
                key: generate_id(prefixes::QUOTATION),
                quotation_number,
                status: STATUS_ACTIVE.to_string(),
                has_out_of_stock_items,
                fulfilled_by_invoice_key: None,
                fulfilled_at: None,
                customer_key,
                customer_name,
                customer_phone,
                customer_address,
                items,
                subtotal_cents,
                discount_type,
                discount_value,
                discount_cents,
                total_cents,
                valid_until: body.valid_until,
                notes,
                terms,
                prepared_by,
                shop_profile_snapshot,
                created_at: now,
                updated_at: now,
                version: 1,
                deleted_at: None,
                updated_by_device: device_id,
            },
        )
        .await?;
        Ok(inserted.into_quotation(&today()))
    })
    .await
}
