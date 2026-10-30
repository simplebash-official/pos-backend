use super::products::products;
use super::row_to_product_doc::row_to_product_doc;

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductDocument;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::options::ReturnDocument;

pub(crate) async fn adjust_product_stock_pipeline(
    unit: impl Into<Unit<'_>>,
    id: ObjectId,
    delta: i64,
) -> AppResult<Option<(i64, ProductDocument)>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let mut filter = doc! { "_id": id };
            if delta < 0 {
                filter.insert("stock_quantity", doc! { "$gte": -delta });
            }

            // Only the stock moves: `stock_quantity` is derived from the
            // stock-movement ledger, so this is not an edit of the product.
            // Bumping `updated_at`/`version` (or the writer) here would let a
            // sale win last-writer-wins over a newer price or name edit made
            // on another device.
            let pipeline_update = vec![doc! {
                "$set": {
                    "stock_quantity": { "$add": ["$stock_quantity", delta] },
                }
            }];

            let result = crate::in_session!(
                products(db)
                    .preserve_origin()
                    .find_one_and_update(filter, pipeline_update)
                    .return_document(ReturnDocument::After),
                session
            )?;

            if let Some(updated) = result {
                let previous_stock = updated.stock_quantity - delta;
                Ok(Some((previous_stock, updated)))
            } else {
                Ok(None)
            }
        }
        Backend::Sqlite(mut conn) => {
            let id_str = id.to_hex();

            // Atomic single-statement update with stock floor guard (stock_quantity + delta >= 0)
            // and RETURNING * to eliminate multi-statement transaction concurrency deadlocks.
            // Only `stock_quantity` changes (see the Mongo arm): the sync
            // capture trigger ignores derived-only writes, so a sale does not
            // re-send the product row.
            let row = sqlx::query(
                r#"
                UPDATE products
                SET stock_quantity = stock_quantity + ?
                WHERE id = ?
                  AND deleted_at IS NULL
                  AND (stock_quantity + ? >= 0)
                RETURNING *
                "#,
            )
            .bind(delta)
            .bind(&id_str)
            .bind(delta)
            .fetch_optional(&mut *conn)
            .await?;

            if let Some(row) = row {
                let updated = row_to_product_doc(&row)?;
                let previous_stock = updated.stock_quantity - delta;
                Ok(Some((previous_stock, updated)))
            } else {
                Ok(None)
            }
        }
    }
}
