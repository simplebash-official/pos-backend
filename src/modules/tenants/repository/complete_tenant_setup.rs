use super::directory::directory;
use crate::clients::db::Db;
use crate::core::error::AppResult;

use mongodb::bson::doc;

/// Marks setup as completed for a tenant.
pub(crate) async fn complete_tenant_setup(
    db: &Db,
    key: &str,
    sample_data_loaded: bool,
) -> AppResult<()> {
    let now = mongodb::bson::DateTime::now();
    directory(db)?
        .update_one(
            doc! { "key": key },
            doc! {
                "$set": {
                    "setup_completed": true,
                    "sample_data_loaded": sample_data_loaded,
                    "setup_completed_at": now,
                    "updated_at": now,
                }
            },
        )
        .await?;
    Ok(())
}
