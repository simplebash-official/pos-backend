use super::built_in_conflict::built_in_conflict;
use super::ensure_name_free::ensure_name_free;

use super::template_not_found::template_not_found;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::labels::{LabelTemplate, LabelTemplateInput};
use crate::modules::labels::model::LabelTemplateDocument;
use crate::modules::labels::repository;

/// Replaces a shop design. A starter is refused (409 `LABEL_TEMPLATE_BUILT_IN`).
pub(crate) async fn update_template(
    db: &Db,
    key: &str,
    input: LabelTemplateInput,
) -> AppResult<LabelTemplate> {
    crate::core::logging::domain::tracked("labels.template_updated", async move {
        if key.starts_with(super::starters::STARTER_KEY_PREFIX) {
            return Err(built_in_conflict());
        }
        super::validate::validate_template(&input)?;
        ensure_name_free(db, &input.name, Some(key)).await?;
        repository::update_template(
            db,
            key,
            input.name.trim(),
            input.width_mm,
            input.height_mm,
            &input.elements,
        )
        .await?
        .map(LabelTemplateDocument::into_template)
        .ok_or_else(template_not_found)
    })
    .await
}
