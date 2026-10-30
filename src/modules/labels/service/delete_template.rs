use super::built_in_conflict::built_in_conflict;

use super::template_not_found::template_not_found;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::labels::repository;

/// Deletes a shop design. A starter is refused (409 `LABEL_TEMPLATE_BUILT_IN`).
pub(crate) async fn delete_template(db: &Db, key: &str) -> AppResult<()> {
    crate::core::logging::domain::tracked("labels.template_deleted", async move {
        if key.starts_with(super::starters::STARTER_KEY_PREFIX) {
            return Err(built_in_conflict());
        }
        if repository::delete_template(db, key).await? {
            Ok(())
        } else {
            Err(template_not_found())
        }
    })
    .await
}
