use super::products::products;
use super::row_to_product_doc::row_to_product_doc;

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductDocument;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn find_product_by_id(
    unit: impl Into<Unit<'_>>,
    id: ObjectId,
) -> AppResult<Option<ProductDocument>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => Ok(crate::in_session!(
            products(db).find_one(doc! { "_id": id, "deleted_at": { "$exists": false } }),
            session
        )?),
        Backend::Sqlite(mut conn) => {
            let row = sqlx::query("SELECT * FROM products WHERE id = ? AND deleted_at IS NULL")
                .bind(id.to_hex())
                .fetch_optional(&mut *conn)
                .await?;

            row.map(|r| row_to_product_doc(&r)).transpose()
        }
    }
}
