use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::repository;
use chrono::{Duration, Utc};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Flips a resolved serial to `Sold`, stamping the invoice it was sold on
/// and computing `warranty_expires_at` from the product's snapshotted
/// `warranty_months` (if any). Called post-commit, mirroring how
/// `apply_stock_delta` runs after the invoice itself is already inserted.
pub(crate) async fn mark_serial_sold(
    db: &Db,
    serial_id: ObjectId,
    invoice_key: &str,
    warranty_months: Option<i64>,
) -> AppResult<()> {
    let now = Utc::now();
    let warranty_expires_at =
        warranty_months.map(|months| BsonDateTime::from_chrono(now + Duration::days(months * 30)));

    let mut set_doc = doc! {
        "status": "sold",
        "invoice_key": invoice_key,
        "sold_at": BsonDateTime::from_chrono(now),
    };
    if let Some(months) = warranty_months {
        set_doc.insert("warranty_months", months);
    }
    if let Some(expires) = warranty_expires_at {
        set_doc.insert("warranty_expires_at", expires);
    }

    repository::product_serial::update_serial(db, serial_id, set_doc).await?;
    Ok(())
}
