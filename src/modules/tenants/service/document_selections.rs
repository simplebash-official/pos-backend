use super::in_shop::in_shop;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::settings;

/// The `(kind, family)` document designs a shop has chosen (operator console).
pub async fn document_selections(db: &Db, tenant_id: &str) -> AppResult<Vec<(String, String)>> {
    in_shop(
        db,
        tenant_id,
        settings::service::list_document_selections(db),
    )
    .await
}
