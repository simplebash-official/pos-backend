use super::directory::directory;
use crate::clients::db::Db;
use crate::core::error::AppResult;

use mongodb::bson::doc;

/// Renames a tenant (identity is the source of truth for the shop name).
pub(crate) async fn update_tenant_name(db: &Db, key: &str, name: &str) -> AppResult<()> {
    directory(db)?
        .update_one(
            doc! { "key": key },
            doc! { "$set": { "name": name, "updated_at": mongodb::bson::DateTime::now() } },
        )
        .await?;
    Ok(())
}
