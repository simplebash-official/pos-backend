use crate::domain::inventory::PriceTier;
use crate::modules::inventory::model::StockBatchDocument;

use std::collections::BTreeMap;

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
