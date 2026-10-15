// Business rules for stock batches — one per delivery of a product, each
// with its own cost and selling price. Owns: creating a batch (always
// together with the movement that brings its units in), grouping in-stock
// batches into the price tiers a cashier picks from, planning which batches a
// sale of N units at a tier draws on (oldest first), restocking returned
// units, and batch price edits. Stock quantities themselves only ever move
// through `service::stock::apply_stock_delta`.

use std::collections::{BTreeMap, HashMap};

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::inventory::{
        BatchAllocation, BatchSource, PriceTier, Product, StockBatch, StockBatchListResponse,
        StockMovementType, UpdateStockBatchRequest,
    },
    modules::inventory::{model::StockBatchDocument, repository},
};

/// `AppError` for a batch key that names no live batch (of the expected product).
pub(crate) fn batch_not_found(key: &str) -> AppError {
    AppError::not_found_with_code(
        format!("Stock batch '{key}' not found"),
        codes::STOCK_BATCH_NOT_FOUND,
    )
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

/// Everything needed to bring a new batch of units into stock.
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

/// Creates a batch and brings its units in. The batch is inserted empty and
/// its units arrive through `apply_stock_delta`, so the batch's remaining
/// quantity, the product total and the ledger agree from the first write —
/// the same shape every later move has, which is what lets sync rebuild the
/// remaining quantity from movements alone.
pub(crate) async fn create_batch(db: &Db, batch: NewBatch) -> AppResult<StockBatch> {
    if batch.quantity < 1 {
        return Err(AppError::validation("Quantity must be at least 1"));
    }
    validate_batch_prices(batch.unit_cost_cents, batch.selling_price_cents)?;

    let now = BsonDateTime::now();
    let inserted = repository::stock_batch::insert_batch(
        db,
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

    super::stock::apply_stock_delta(
        db,
        batch.product_id,
        Some(&inserted.key),
        batch.quantity,
        batch.movement_type,
        batch.reference_id,
        batch.note,
    )
    .await?;

    let stored = repository::stock_batch::find_batch_by_key(db, &inserted.key)
        .await?
        .ok_or_else(|| batch_not_found(&inserted.key))?;
    Ok(stored.into_stock_batch())
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
    for product in products {
        product.price_tiers = by_product
            .get(product.key.as_str())
            .map(|batches| tiers_from_batches(batches.iter().copied()))
            .unwrap_or_default();
    }
    Ok(())
}

/// `attach_price_tiers` for a single product.
pub(crate) async fn with_price_tiers(db: &Db, product: Product) -> AppResult<Product> {
    let mut products = [product];
    attach_price_tiers(db, &mut products).await?;
    let [product] = products;
    Ok(product)
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
        super::stock::apply_stock_delta(
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

/// All batches of a product, newest first (`GET /products/{key}/batches`).
pub(crate) async fn list_batches(db: &Db, product_key: &str) -> AppResult<StockBatchListResponse> {
    super::product::get_product_by_key(db, product_key).await?;
    let items = repository::stock_batch::list_batches_for_product(db, product_key)
        .await?
        .into_iter()
        .map(StockBatchDocument::into_stock_batch)
        .collect();
    Ok(StockBatchListResponse { items })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(key: &str, price: i64, cost: i64, remaining: i64, day: i64) -> StockBatchDocument {
        StockBatchDocument {
            id: Some(ObjectId::new()),
            key: key.to_string(),
            product_key: "prod_a".to_string(),
            source: BatchSource::Purchase,
            purchase_key: None,
            supplier_key: None,
            received_quantity: remaining,
            remaining_quantity: remaining,
            unit_cost_cents: cost,
            selling_price_cents: price,
            received_at: BsonDateTime::from_millis(day * 86_400_000),
            version: 1,
            created_at: BsonDateTime::from_millis(0),
            updated_at: BsonDateTime::from_millis(0),
            deleted_at: None,
            updated_by_device: None,
        }
    }

    // Item A: 5 left of the Rs.1000/1200 delivery, 12 of the Rs.1100/1500 one.
    fn item_a() -> Vec<StockBatchDocument> {
        vec![
            batch("stb_1", 120_000, 100_000, 5, 1),
            batch("stb_2", 150_000, 110_000, 12, 2),
        ]
    }

    #[test]
    fn each_selling_price_is_its_own_tier() {
        let tiers = tiers_from_batches(&item_a());
        assert_eq!(tiers.len(), 2);
        assert_eq!(
            (tiers[0].selling_price_cents, tiers[0].quantity),
            (120_000, 5)
        );
        assert_eq!(
            (tiers[1].selling_price_cents, tiers[1].quantity),
            (150_000, 12)
        );
        assert_eq!(tiers[1].min_unit_cost_cents, Some(110_000));
    }

    #[test]
    fn same_price_batches_merge_and_empty_ones_drop_out() {
        let batches = vec![
            batch("stb_1", 150_000, 100_000, 3, 1),
            batch("stb_2", 150_000, 110_000, 4, 2),
            batch("stb_3", 120_000, 90_000, 0, 3),
        ];
        let tiers = tiers_from_batches(&batches);
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0].quantity, 7);
        assert_eq!(tiers[0].min_unit_cost_cents, Some(100_000));
        assert_eq!(tiers[0].max_unit_cost_cents, Some(110_000));
        assert_eq!(
            tiers[0].oldest_received_at,
            BsonDateTime::from_millis(86_400_000).to_chrono()
        );
    }

    #[test]
    fn a_sale_draws_only_on_its_tier() {
        let plan = plan_from_batches(&item_a(), "Item A", 150_000, 3).unwrap();
        assert_eq!(
            plan,
            vec![BatchAllocation {
                batch_key: "stb_2".into(),
                quantity: 3,
                unit_cost_cents: 110_000
            }]
        );
    }

    #[test]
    fn a_tier_is_consumed_oldest_first_across_batches() {
        let batches = vec![
            batch("stb_old", 150_000, 100_000, 2, 1),
            batch("stb_new", 150_000, 110_000, 10, 2),
        ];
        let plan = plan_from_batches(&batches, "Item A", 150_000, 5).unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(
            (plan[0].batch_key.as_str(), plan[0].quantity),
            ("stb_old", 2)
        );
        assert_eq!(
            (plan[1].batch_key.as_str(), plan[1].quantity),
            ("stb_new", 3)
        );
        assert_eq!(weighted_unit_cost(&plan), (2 * 100_000 + 3 * 110_000) / 5);
    }

    #[test]
    fn asking_for_more_than_the_tier_holds_is_refused() {
        let err = plan_from_batches(&item_a(), "Item A", 120_000, 6).unwrap_err();
        assert!(format!("{err:?}").contains("INSUFFICIENT_STOCK"));
    }

    #[test]
    fn returns_go_back_newest_batch_first_and_continue_where_the_last_stopped() {
        let line = vec![
            BatchAllocation {
                batch_key: "stb_old".into(),
                quantity: 2,
                unit_cost_cents: 100,
            },
            BatchAllocation {
                batch_key: "stb_new".into(),
                quantity: 3,
                unit_cost_cents: 110,
            },
        ];
        let first = allocations_for_return(&line, 0, 2);
        assert_eq!(
            first,
            vec![BatchAllocation {
                batch_key: "stb_new".into(),
                quantity: 2,
                unit_cost_cents: 110
            }]
        );
        let second = allocations_for_return(&line, 2, 3);
        assert_eq!(
            second,
            vec![
                BatchAllocation {
                    batch_key: "stb_new".into(),
                    quantity: 1,
                    unit_cost_cents: 110
                },
                BatchAllocation {
                    batch_key: "stb_old".into(),
                    quantity: 2,
                    unit_cost_cents: 100
                },
            ]
        );
    }

    #[test]
    fn migration_gives_old_product_lines_their_opening_batch() {
        let mut items = serde_json::json!([
            { "productKey": "prod_a", "quantity": 2, "unitCostCents": 900 },
            { "productKey": "prod_b", "quantity": 1 },
            { "name": "Screen repair", "quantity": 1, "sourceType": "repair" },
            { "productKey": "prod_c", "quantity": 1,
              "batchAllocations": [{ "batchKey": "stb_x", "quantity": 1, "unitCostCents": 5 }] },
        ]);
        let changed = backfill_line_allocations(&mut items, |key| (key == "prod_b").then_some(700));
        assert!(changed);
        assert_eq!(
            items[0]["batchAllocations"][0]["batchKey"],
            "stb_open_prod_a"
        );
        assert_eq!(items[0]["batchAllocations"][0]["unitCostCents"], 900);
        assert_eq!(items[1]["batchAllocations"][0]["unitCostCents"], 700);
        assert!(items[2].get("batchAllocations").is_none());
        assert_eq!(items[3]["batchAllocations"][0]["batchKey"], "stb_x");
        assert!(!backfill_line_allocations(&mut items, |_| None));
    }

    #[test]
    fn a_price_no_batch_sells_at_is_refused() {
        let err = plan_from_batches(&item_a(), "Item A", 130_000, 1).unwrap_err();
        assert!(format!("{err:?}").contains("PRICE_TIER_UNAVAILABLE"));
    }
}
