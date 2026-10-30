use super::row_to_batch_doc::row_to_batch_doc;
use super::stock_batches::stock_batches;

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;
use crate::modules::inventory::model::StockBatchDocument;

use mongodb::bson::doc;

/// Fetches one live batch by its key.
pub(crate) async fn find_batch_by_key(
    unit: impl Into<Unit<'_>>,
    key: &str,
) -> AppResult<Option<StockBatchDocument>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => Ok(crate::in_session!(
            stock_batches(db).find_one(doc! { "key": key, "deleted_at": { "$exists": false } }),
            session
        )?),
        Backend::Sqlite(mut conn) => {
            let row =
                sqlx::query("SELECT * FROM stock_batches WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(&mut *conn)
                    .await?;
            row.map(|r| row_to_batch_doc(&r)).transpose()
        }
    }
}
