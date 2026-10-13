// Business rules for quotations: validation, server-side totals, numbering
// and the frozen shop-profile snapshot. A quotation never calls into stock,
// payments, invoices or reports — it is a record of an offer, nothing more.

use chrono::{NaiveDate, Utc};
use mongodb::bson::{self, DateTime as BsonDateTime, Document};

use crate::{
    clients::db::Db,
    core::{
        calculations::{compute_line_total, compute_order_discount},
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        utils::calculate_pagination,
    },
    domain::{
        billing::PricingAdjustments,
        quotations::{
            CreateQuotationItem, CreateQuotationRequest, Quotation, QuotationItem,
            QuotationListQuery, QuotationListResponse, STATUS_ACTIVE, SyncQuotation,
        },
        sequences::ReserveSequenceRequest,
    },
    modules::{
        customers, inventory,
        quotations::{
            model::QuotationDocument,
            repository::{self, ListFilter},
        },
        sequences, settings,
    },
};

const MAX_ITEMS: usize = 200;
const MAX_NAME_LEN: usize = 200;
const MAX_TEXT_LEN: usize = 2000;
const LIST_STATUSES: &[&str] = &["active", "expired", "cancelled"];

/// `YYYY-MM-DD` for today, UTC — the same day boundary every other module uses.
fn today() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

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
            (
                Some(product.key),
                product.name,
                Some(product.sku),
                product.selling_price_cents,
            )
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

pub async fn list_quotations(
    db: &Db,
    query: QuotationListQuery,
) -> AppResult<QuotationListResponse> {
    let status = query
        .status
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(s) = &status
        && !LIST_STATUSES.contains(&s.as_str())
    {
        return Err(AppError::validation(format!(
            "Invalid status '{s}'. Must be one of: {}",
            LIST_STATUSES.join(", ")
        )));
    }
    let search = query
        .search
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let today = today();
    let filter = ListFilter {
        search,
        status,
        today: today.clone(),
    };
    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 200);
    let (documents, total) = repository::list(db, &filter, skip, limit).await?;

    Ok(QuotationListResponse {
        quotations: documents
            .into_iter()
            .map(|d| d.into_quotation(&today))
            .collect(),
        total,
        page,
        limit,
        total_pages: total.div_ceil(limit),
    })
}

async fn find_document(db: &Db, id_or_key: &str) -> AppResult<QuotationDocument> {
    repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Quotation not found", codes::QUOTATION_NOT_FOUND)
        })
}

pub async fn get_quotation(db: &Db, id_or_key: &str) -> AppResult<Quotation> {
    Ok(find_document(db, id_or_key).await?.into_quotation(&today()))
}

/// The quotation plus its frozen shop profile, for the PDF builder only.
pub(crate) async fn get_quotation_for_print(
    db: &Db,
    id_or_key: &str,
) -> AppResult<(Quotation, serde_json::Value)> {
    let document = find_document(db, id_or_key).await?;
    let shop = document.shop_profile_snapshot.clone();
    Ok((document.into_quotation(&today()), shop))
}

/// Cancels an active quotation. A cancelled quotation stays printable (the
/// PDF is stamped CANCELLED) so a paper copy still in a customer's hands can
/// be checked against the record.
pub async fn cancel_quotation(
    db: &Db,
    id_or_key: &str,
    device_id: Option<String>,
) -> AppResult<Quotation> {
    crate::core::logging::domain::tracked("quotations.cancelled", async move {
        let existing = find_document(db, id_or_key).await?;
        let object_id = existing
            .id
            .expect("persisted quotation document must have an _id");
        match repository::cancel(db, object_id, device_id).await? {
            Some(updated) => Ok(updated.into_quotation(&today())),
            None => Err(AppError::conflict(
                codes::QUOTATION_ALREADY_CANCELLED,
                "This quotation is already cancelled",
            )),
        }
    })
    .await
}

/// Converts a page of raw `quotations` documents — as read by the sync module's
/// cursor scan — into the wire shape devices and the cloud exchange. Every sync
/// feed (pull, snapshot, push, outbox) goes through `sync::service::hydrate`,
/// and a resource without an arm there silently syncs nothing.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<SyncQuotation>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<QuotationDocument>(document)?
                    .into_sync_quotation(),
            )
        })
        .collect()
}
