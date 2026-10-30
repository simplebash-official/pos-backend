use super::row_to_batch_doc::row_to_batch_doc;
use super::stock_batches::stock_batches;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;
use crate::modules::inventory::model::StockBatchDocument;

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use mongodb::options::ReturnDocument;

/// Applies an edit of a batch's own (non-derived) fields — selling price
/// and/or cost — bumping `version`/`updated_at` so the change travels
/// last-writer-wins like any other edit.
pub(crate) async fn update_batch(
    db: &Db,
    key: &str,
    selling_price_cents: Option<i64>,
    unit_cost_cents: Option<i64>,
) -> AppResult<Option<StockBatchDocument>> {
    let now = BsonDateTime::now();
    match db {
        Db::Mongo(db) => {
            let mut set: Document = doc! { "updated_at": now };
            if let Some(price) = selling_price_cents {
                set.insert("selling_price_cents", price);
            }
            if let Some(cost) = unit_cost_cents {
                set.insert("unit_cost_cents", cost);
            }
            Ok(stock_batches(db)
                .find_one_and_update(
                    doc! { "key": key, "deleted_at": { "$exists": false } },
                    doc! { "$set": set, "$inc": { "version": 1 } },
                )
                .return_document(ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                r#"
                UPDATE stock_batches
                SET selling_price_cents = COALESCE(?, selling_price_cents),
                    unit_cost_cents = COALESCE(?, unit_cost_cents),
                    updated_at = ?, version = version + 1
                WHERE key = ? AND deleted_at IS NULL
                RETURNING *
                "#,
            )
            .bind(selling_price_cents)
            .bind(unit_cost_cents)
            .bind(bson_to_iso(&now))
            .bind(key)
            .fetch_optional(pool)
            .await?;
            row.map(|r| row_to_batch_doc(&r)).transpose()
        }
    }
}
