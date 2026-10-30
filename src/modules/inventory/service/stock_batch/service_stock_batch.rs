use crate::clients::db::Db;
use crate::clients::uow::Unit;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::inventory::{
    BatchAllocation, BatchSource, PriceTier, Product, StockBatch, StockBatchListResponse,
    StockMovementType, UpdateStockBatchRequest,
};
use crate::modules::inventory::model::StockBatchDocument;
use crate::modules::inventory::repository;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use std::collections::{BTreeMap, HashMap};

/// Which batches `quantity` returned units of a sold line go back into: the
/// line's own batches, newest allocation first, skipping the units earlier
/// returns already put back (`already_returned`). Pure and deterministic, so
/// a later return of the same line continues where the previous one stopped.
pub(crate) fn allocations_for_return(
    line_allocations: &[BatchAllocation],
    already_returned: i64,
    quantity: i64,
) -> Vec<BatchAllocation> {
    let mut skip = already_returned.max(0);
    let mut left = quantity;
    let mut result = Vec::new();
    for allocation in line_allocations.iter().rev() {
        if left == 0 {
            break;
        }
        let usable = allocation.quantity - skip.min(allocation.quantity);
        skip -= skip.min(allocation.quantity);
        if usable == 0 {
            continue;
        }
        let take = usable.min(left);
        result.push(BatchAllocation {
            batch_key: allocation.batch_key.clone(),
            quantity: take,
            unit_cost_cents: allocation.unit_cost_cents,
        });
        left -= take;
    }
    result
}

/// Price and batch allocations of a serialized line: each unit sells at its
/// own batch's price, so the chosen serials must all come from batches at
/// one price (a cart line has one unit price). `batch_keys` is each chosen
/// serial's batch, one entry per unit.
pub(crate) async fn allocations_for_serials(
    db: &Db,
    product_key: &str,
    product_name: &str,
    batch_keys: &[String],
) -> AppResult<(i64, Vec<BatchAllocation>)> {
    let mut allocations: Vec<BatchAllocation> = Vec::new();
    let mut price: Option<i64> = None;
    for batch_key in batch_keys {
        if let Some(existing) = allocations.iter_mut().find(|a| &a.batch_key == batch_key) {
            existing.quantity += 1;
            continue;
        }
        let batch = get_batch_for_product(db, batch_key, product_key).await?;
        match price {
            Some(p) if p != batch.selling_price_cents => {
                return Err(AppError::validation(format!(
                    "The chosen units of {product_name} sell at different prices — put each price on its own line"
                )));
            }
            _ => price = Some(batch.selling_price_cents),
        }
        allocations.push(BatchAllocation {
            batch_key: batch.key,
            quantity: 1,
            unit_cost_cents: batch
                .unit_cost_cents
                .expect("a batch read from the store carries its cost"),
        });
    }
    let price =
        price.ok_or_else(|| AppError::validation("A serialized line needs its serial numbers"))?;
    Ok((price, allocations))
}

/// Fills `price_tiers` on every product from its in-stock batches, with one
/// batch query for the whole slice. Every product read path calls this
/// before answering (see `ProductDocument::into_product`).
pub(crate) async fn attach_price_tiers<'a>(
    db: &Db,
    products: impl IntoIterator<Item = &'a mut Product>,
) -> AppResult<()> {
    let products: Vec<&mut Product> = products.into_iter().collect();
    let keys: Vec<String> = products.iter().map(|product| product.key.clone()).collect();
    let batches = repository::stock_batch::list_in_stock_batches_for_products(db, &keys).await?;
    let mut by_product: HashMap<&str, Vec<&StockBatchDocument>> = HashMap::new();
    for batch in &batches {
        by_product
            .entry(batch.product_key.as_str())
            .or_default()
            .push(batch);
    }

    let mut out_of_stock_keys: Vec<String> = Vec::new();
    for product in &products {
        if !by_product.contains_key(product.key.as_str()) {
            out_of_stock_keys.push(product.key.clone());
        }
    }

    let latest_batches = if !out_of_stock_keys.is_empty() {
        repository::stock_batch::list_latest_batches_for_products(db, &out_of_stock_keys).await?
    } else {
        Vec::new()
    };
    let mut latest_by_product: HashMap<&str, i64> = HashMap::new();
    for batch in &latest_batches {
        latest_by_product
            .entry(batch.product_key.as_str())
            .or_insert(batch.selling_price_cents);
    }

    for product in products {
        let tiers = by_product
            .get(product.key.as_str())
            .map(|batches| tiers_from_batches(batches.iter().copied()))
            .unwrap_or_default();
        if let Some(last_tier) = tiers.last() {
            product.last_selling_price_cents = Some(last_tier.selling_price_cents);
        } else if let Some(last_price) = latest_by_product.get(product.key.as_str()) {
            product.last_selling_price_cents = Some(*last_price);
        }
        product.price_tiers = tiers;
    }
    Ok(())
}

/// One-time migration to stock batches, for a list of stored sale lines
/// (an invoice's `items`, a credit note's `returnedItems`/`exchangeItems`,
/// camelCase JSON): gives every product line that has none a single
/// allocation from the product's opening batch
/// (`prefixes::opening_batch_key`), at the line's recorded unit cost or, when
/// the line never recorded one, the product's last cost (`cost_of`). Returns
/// whether anything changed. Used by the SQLite migration
/// (`clients::sqlite::migrate_to_stock_batches`) and the Mongo one
/// (`bin/migrate_stock_batches.rs`) so both rewrite history identically.
pub fn backfill_line_allocations(
    items: &mut serde_json::Value,
    cost_of: impl Fn(&str) -> Option<i64>,
) -> bool {
    let Some(lines) = items.as_array_mut() else {
        return false;
    };
    let mut changed = false;
    for line in lines {
        let Some(product_key) = line.get("productKey").and_then(|v| v.as_str()) else {
            continue;
        };
        let has_allocations = line
            .get("batchAllocations")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty());
        let quantity = line.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0);
        if has_allocations || quantity <= 0 {
            continue;
        }
        let Some(unit_cost_cents) = line
            .get("unitCostCents")
            .and_then(|v| v.as_i64())
            .or_else(|| cost_of(product_key))
        else {
            continue;
        };
        let batch_key = crate::core::constants::prefixes::opening_batch_key(product_key);
        line["batchAllocations"] = serde_json::json!([{
            "batchKey": batch_key,
            "quantity": quantity,
            "unitCostCents": unit_cost_cents,
        }]);
        changed = true;
    }
    changed
}

/// `AppError` for a batch key that names no live batch (of the expected product).
pub(crate) fn batch_not_found(key: &str) -> AppError {
    AppError::not_found_with_code(
        format!("Stock batch '{key}' not found"),
        codes::STOCK_BATCH_NOT_FOUND,
    )
}

/// Creates a batch and brings its units in. The batch is inserted empty and
/// its units arrive through `apply_stock_delta`, so the batch's remaining
/// quantity, the product total and the ledger agree from the first write —
/// the same shape every later move has, which is what lets sync rebuild the
/// remaining quantity from movements alone.
pub(crate) async fn create_batch(
    unit: impl Into<Unit<'_>>,
    batch: NewBatch,
) -> AppResult<StockBatch> {
    let mut unit = unit.into();
    if batch.quantity < 1 {
        return Err(AppError::validation("Quantity must be at least 1"));
    }
    validate_batch_prices(batch.unit_cost_cents, batch.selling_price_cents)?;

    let now = BsonDateTime::now();
    let inserted = repository::stock_batch::insert_batch(
        &mut unit,
        StockBatchDocument {
            id: None,
            key: generate_id(prefixes::STOCK_BATCH),
            product_key: batch.product_key,
            source: batch.source,
            purchase_key: batch.purchase_key,
            supplier_key: batch.supplier_key,
            received_quantity: batch.quantity,
            remaining_quantity: 0,
            unit_cost_cents: batch.unit_cost_cents,
            selling_price_cents: batch.selling_price_cents,
            received_at: batch.received_at,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        },
    )
    .await?;

    super::super::stock::apply_stock_delta(
        &mut unit,
        batch.product_id,
        Some(&inserted.key),
        batch.quantity,
        batch.movement_type,
        batch.reference_id,
        batch.note,
    )
    .await?;

    let stored = repository::stock_batch::find_batch_by_key(&mut unit, &inserted.key)
        .await?
        .ok_or_else(|| batch_not_found(&inserted.key))?;
    Ok(stored.into_stock_batch())
}

/// Loads one live batch, requiring it to belong to `product_key`.
pub(crate) async fn get_batch_for_product(
    db: &Db,
    batch_key: &str,
    product_key: &str,
) -> AppResult<StockBatch> {
    repository::stock_batch::find_batch_by_key(db, batch_key)
        .await?
        .filter(|batch| batch.product_key == product_key)
        .map(StockBatchDocument::into_stock_batch)
        .ok_or_else(|| batch_not_found(batch_key))
}

/// Converts a page of raw `stock_batches` documents — as read by the sync
/// module's cursor scan — into the `StockBatch` shape the REST reads return.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<StockBatch>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<StockBatchDocument>(document)?.into_stock_batch())
        })
        .collect()
}

/// Units still on the shelf from one batch, and when that batch was entered.
/// What `labels` needs to tell which units arrived after a product's last label.
#[derive(Debug, Clone)]
pub(crate) struct InStockUnits {
    pub product_key: String,
    pub remaining_quantity: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// The in-stock units of the given products, one entry per batch. Uses the
/// batch's `created_at` (when it was entered), not its `received_at`, which a
/// person may backdate.
pub(crate) async fn in_stock_units(
    db: &Db,
    product_keys: &[String],
) -> AppResult<Vec<InStockUnits>> {
    Ok(
        repository::stock_batch::list_in_stock_batches_for_products(db, product_keys)
            .await?
            .into_iter()
            .map(|batch| InStockUnits {
                product_key: batch.product_key,
                remaining_quantity: batch.remaining_quantity,
                created_at: batch.created_at.to_chrono(),
            })
            .collect(),
    )
}

/// All batches of a product, newest first (`GET /products/{key}/batches`).
pub(crate) async fn list_batches(db: &Db, product_key: &str) -> AppResult<StockBatchListResponse> {
    super::super::product::get_product_by_key(db, product_key).await?;
    let items = repository::stock_batch::list_batches_for_product(db, product_key)
        .await?
        .into_iter()
        .map(StockBatchDocument::into_stock_batch)
        .collect();
    Ok(StockBatchListResponse { items })
}

/// Moves `allocations` in (`sign = 1`) or out (`sign = -1`) of their batches,
/// one movement per batch, all tagged with `movement_type`/`reference_id` —
/// how sales, voids, returns and exchanges touch stock. Stops at the first
/// failure and returns it; callers decide whether that fails the request.
pub(crate) async fn move_allocations(
    db: &Db,
    product_key: &str,
    allocations: &[BatchAllocation],
    sign: i64,
    movement_type: StockMovementType,
    reference_id: &str,
    note: &str,
) -> AppResult<()> {
    let product = repository::product::find_product_by_key(db, product_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;
    let product_id = product
        .id
        .expect("persisted product document must have an id");
    for allocation in allocations {
        super::super::stock::apply_stock_delta(
            db,
            product_id,
            Some(&allocation.batch_key),
            sign * allocation.quantity,
            movement_type,
            Some(reference_id.to_string()),
            Some(note.to_string()),
        )
        .await?;
    }
    Ok(())
}

/// Everything needed to bring a new batch of units into stock.
#[derive(Debug)]
pub(crate) struct NewBatch {
    /// The product's `_id` (stock movements reference products by it).
    pub product_id: ObjectId,
    pub product_key: String,
    pub source: BatchSource,
    pub purchase_key: Option<String>,
    pub supplier_key: Option<String>,
    pub quantity: i64,
    pub unit_cost_cents: i64,
    pub selling_price_cents: i64,
    pub received_at: BsonDateTime,
    /// How the incoming movement is tagged (`PurchaseReceipt`, `OpeningBalance`, ...).
    pub movement_type: StockMovementType,
    pub reference_id: Option<String>,
    pub note: Option<String>,
}

/// The batch a no-receipt return at `selling_price_cents` goes back into:
/// the newest batch of the product at that price (empty ones included, so a
/// unit returned after its tier sold out still rejoins its own delivery).
/// `None` when the product has never been sold at that price.
pub(crate) async fn newest_batch_at_price(
    db: &Db,
    product_key: &str,
    selling_price_cents: i64,
) -> AppResult<Option<StockBatch>> {
    Ok(
        repository::stock_batch::list_batches_for_product(db, product_key)
            .await?
            .into_iter()
            .find(|batch| batch.selling_price_cents == selling_price_cents)
            .map(StockBatchDocument::into_stock_batch),
    )
}

/// Splits a sale of `quantity` units at `selling_price_cents` across the
/// in-stock batches of that price, oldest first. Pure, so the rule is tested
/// without a database. Errors when the tier doesn't exist
/// (`PRICE_TIER_UNAVAILABLE`) or holds fewer units than asked
/// (`INSUFFICIENT_STOCK`), before anything is written.
pub(crate) fn plan_from_batches<'a>(
    batches: impl IntoIterator<Item = &'a StockBatchDocument>,
    product_name: &str,
    selling_price_cents: i64,
    quantity: i64,
) -> AppResult<Vec<BatchAllocation>> {
    if quantity < 1 {
        return Err(AppError::validation("Quantity must be at least 1"));
    }
    let tier: Vec<&StockBatchDocument> = batches
        .into_iter()
        .filter(|batch| {
            batch.selling_price_cents == selling_price_cents && batch.remaining_quantity > 0
        })
        .collect();
    if tier.is_empty() {
        return Err(AppError::conflict_with_details(
            codes::PRICE_TIER_UNAVAILABLE,
            format!("{product_name} is not in stock at this price any more."),
            serde_json::json!({ "sellingPriceCents": selling_price_cents }),
        ));
    }
    let available: i64 = tier.iter().map(|batch| batch.remaining_quantity).sum();
    if available < quantity {
        return Err(AppError::conflict_with_details(
            codes::INSUFFICIENT_STOCK,
            format!(
                "Only {available} unit{} of {product_name} {} left at this price (asked for {quantity}).",
                if available == 1 { "" } else { "s" },
                if available == 1 { "is" } else { "are" },
            ),
            serde_json::json!({
                "available": available,
                "requested": quantity,
                "sellingPriceCents": selling_price_cents,
            }),
        ));
    }

    let mut left = quantity;
    let mut allocations = Vec::new();
    for batch in tier {
        if left == 0 {
            break;
        }
        let take = left.min(batch.remaining_quantity);
        allocations.push(BatchAllocation {
            batch_key: batch.key.clone(),
            quantity: take,
            unit_cost_cents: batch.unit_cost_cents,
        });
        left -= take;
    }
    Ok(allocations)
}

/// `plan_from_batches` against the product's live in-stock batches.
pub(crate) async fn plan_tier_allocation(
    db: &Db,
    product_key: &str,
    product_name: &str,
    selling_price_cents: i64,
    quantity: i64,
) -> AppResult<Vec<BatchAllocation>> {
    let batches =
        repository::stock_batch::list_in_stock_batches_for_products(db, &[product_key.to_string()])
            .await?;
    plan_from_batches(&batches, product_name, selling_price_cents, quantity)
}

/// Groups in-stock batches (already sorted oldest first) into price tiers:
/// one per distinct selling price, cheapest first, each carrying its total
/// quantity, its cost range and its oldest receipt date.
pub(crate) fn tiers_from_batches<'a>(
    batches: impl IntoIterator<Item = &'a StockBatchDocument>,
) -> Vec<PriceTier> {
    let mut tiers: BTreeMap<i64, PriceTier> = BTreeMap::new();
    for batch in batches {
        if batch.remaining_quantity <= 0 {
            continue;
        }
        let received_at = batch.received_at.to_chrono();
        tiers
            .entry(batch.selling_price_cents)
            .and_modify(|tier| {
                tier.quantity += batch.remaining_quantity;
                tier.min_unit_cost_cents = tier
                    .min_unit_cost_cents
                    .map(|c| c.min(batch.unit_cost_cents));
                tier.max_unit_cost_cents = tier
                    .max_unit_cost_cents
                    .map(|c| c.max(batch.unit_cost_cents));
                tier.oldest_received_at = tier.oldest_received_at.min(received_at);
            })
            .or_insert(PriceTier {
                selling_price_cents: batch.selling_price_cents,
                quantity: batch.remaining_quantity,
                min_unit_cost_cents: Some(batch.unit_cost_cents),
                max_unit_cost_cents: Some(batch.unit_cost_cents),
                oldest_received_at: received_at,
            });
    }
    tiers.into_values().collect()
}

/// Edits a batch's selling price and/or cost (`PATCH /batches/{key}`).
/// Only an Admin may change the cost; the caller passes `may_edit_cost`.
pub(crate) async fn update_batch(
    db: &Db,
    key: &str,
    body: UpdateStockBatchRequest,
    may_edit_cost: bool,
) -> AppResult<StockBatch> {
    crate::core::logging::domain::tracked("inventory.stock_batch_updated", async move {
        if body.selling_price_cents.is_none() && body.unit_cost_cents.is_none() {
            return Err(AppError::validation(
                "Send sellingPriceCents and/or unitCostCents to change",
            ));
        }
        if body.unit_cost_cents.is_some() && !may_edit_cost {
            return Err(AppError::forbidden_with_code(
                "Only an Admin can change a batch's cost",
                codes::ADMIN_REQUIRED,
            ));
        }
        let existing = repository::stock_batch::find_batch_by_key(db, key)
            .await?
            .ok_or_else(|| batch_not_found(key))?;
        validate_batch_prices(
            body.unit_cost_cents.unwrap_or(existing.unit_cost_cents),
            body.selling_price_cents
                .unwrap_or(existing.selling_price_cents),
        )?;
        repository::stock_batch::update_batch(
            db,
            key,
            body.selling_price_cents,
            body.unit_cost_cents,
        )
        .await?
        .map(StockBatchDocument::into_stock_batch)
        .ok_or_else(|| batch_not_found(key))
    })
    .await
}

/// A batch's prices must be usable: something to sell at, and a cost that
/// isn't negative. Same rule a product's prices followed before batches.
pub(crate) fn validate_batch_prices(
    unit_cost_cents: i64,
    selling_price_cents: i64,
) -> AppResult<()> {
    if selling_price_cents <= 0 {
        return Err(AppError::validation("Selling price must be greater than 0"));
    }
    if unit_cost_cents < 0 {
        return Err(AppError::validation("Unit cost cannot be negative"));
    }
    Ok(())
}

/// Weighted unit cost of an allocation set, floored — what an invoice line
/// records as its `unit_cost_cents` so the profit reports read the real cost.
pub(crate) fn weighted_unit_cost(allocations: &[BatchAllocation]) -> i64 {
    let units: i64 = allocations.iter().map(|a| a.quantity).sum();
    if units == 0 {
        return 0;
    }
    allocations
        .iter()
        .map(|a| a.quantity * a.unit_cost_cents)
        .sum::<i64>()
        / units
}

/// `attach_price_tiers` for a single product.
pub(crate) async fn with_price_tiers(db: &Db, product: Product) -> AppResult<Product> {
    let mut products = [product];
    attach_price_tiers(db, &mut products).await?;
    let [product] = products;
    Ok(product)
}
