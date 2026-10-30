use super::row_to_batch_doc::row_to_batch_doc;
use super::stock_batches::stock_batches;

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;
use crate::modules::inventory::model::StockBatchDocument;

use mongodb::bson::doc;
use mongodb::options::ReturnDocument;

/// Moves a batch's derived `remaining_quantity` by `delta` in one atomic
/// write, refusing (returning `None`) when that would take it below zero —
/// the guard and the write are one statement, so two concurrent sales of
/// the last unit cannot both pass. Like the product stock column, only the
/// derived value changes: no `version`/`updated_at` bump, writer preserved.
pub(crate) async fn adjust_batch_remaining(
    unit: impl Into<Unit<'_>>,
    key: &str,
    delta: i64,
) -> AppResult<Option<StockBatchDocument>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let mut filter = doc! { "key": key, "deleted_at": { "$exists": false } };
            if delta < 0 {
                filter.insert("remaining_quantity", doc! { "$gte": -delta });
            }
            let update = vec![doc! {
                "$set": { "remaining_quantity": { "$add": ["$remaining_quantity", delta] } }
            }];
            Ok(crate::in_session!(
                stock_batches(db)
                    .preserve_origin()
                    .find_one_and_update(filter, update)
                    .return_document(ReturnDocument::After),
                session
            )?)
        }
        Backend::Sqlite(mut conn) => {
            let row = sqlx::query(
                r#"
                UPDATE stock_batches
                SET remaining_quantity = remaining_quantity + ?
                WHERE key = ? AND deleted_at IS NULL AND remaining_quantity + ? >= 0
                RETURNING *
                "#,
            )
            .bind(delta)
            .bind(key)
            .bind(delta)
            .fetch_optional(&mut *conn)
            .await?;
            row.map(|r| row_to_batch_doc(&r)).transpose()
        }
    }
}
