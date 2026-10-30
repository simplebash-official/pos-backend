use crate::clients::tenant_db::TenantDatabase;
use crate::core::error::AppResult;

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

/// Marks the parent category changed. A subcategory travels folded into its
/// category, so devices only learn about a subcategory change through a newer
/// version of the category (the device-side capture triggers do the same).
pub(super) async fn touch_parent_category(
    db: &TenantDatabase,
    category_key: &str,
) -> AppResult<()> {
    db.collection::<Document>("categories")
        .update_one(
            doc! { "key": category_key },
            doc! { "$set": { "updated_at": BsonDateTime::now() }, "$inc": { "version": 1 } },
        )
        .await?;
    Ok(())
}
