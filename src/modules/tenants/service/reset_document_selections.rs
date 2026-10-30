use super::in_shop::in_shop;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::settings;

/// Clears the shop's choices of the given design families, and the option
/// values it saved for them; returns the `(kind, family)` choices removed.
pub async fn reset_document_selections(
    db: &Db,
    tenant_id: &str,
    families: &[String],
) -> AppResult<Vec<(String, String)>> {
    in_shop(
        db,
        tenant_id,
        settings::service::reset_document_families(db, families),
    )
    .await
}
