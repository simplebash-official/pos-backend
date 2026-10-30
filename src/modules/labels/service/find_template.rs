use super::template_not_found::template_not_found;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::labels::LabelTemplate;
use crate::modules::labels::model::LabelTemplateDocument;
use crate::modules::labels::repository;

/// A design by key: a starter (`starter_*`) or one the shop built (`ltpl_*`).
pub(super) async fn find_template(db: &Db, key: &str) -> AppResult<LabelTemplate> {
    if key.starts_with(super::starters::STARTER_KEY_PREFIX) {
        return super::starters::find(key).ok_or_else(template_not_found);
    }
    repository::find_template_by_key(db, key)
        .await?
        .map(LabelTemplateDocument::into_template)
        .ok_or_else(template_not_found)
}
