use super::min_selling_price::MIN_SELLING_PRICE;

use mongodb::bson::{Document, doc};

/// Same-module `$lookup` adding `min_selling_price_cents` (the lowest
/// selling price among the product's in-stock batches) so a `$sort` on it
/// works. Product keys are globally unique, so the join can't cross tenants.
pub(super) fn min_price_stages() -> Vec<Document> {
    vec![
        doc! { "$lookup": {
            "from": "stock_batches",
            "localField": "key",
            "foreignField": "product_key",
            "as": "_price_batches",
        } },
        doc! { "$addFields": { MIN_SELLING_PRICE: { "$min": { "$map": {
            "input": { "$filter": {
                "input": "$_price_batches",
                "cond": { "$and": [
                    { "$gt": ["$$this.remaining_quantity", 0] },
                    { "$not": ["$$this.deleted_at"] },
                ] },
            } },
            "in": "$$this.selling_price_cents",
        } } } } },
        doc! { "$project": { "_price_batches": 0 } },
    ]
}
