use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::labels::LabelTemplatesResponse;
use crate::modules::labels::model::LabelTemplateDocument;
use crate::modules::labels::repository;

/// The starters, then the shop's own designs by name.
pub(crate) async fn list_templates(db: &Db) -> AppResult<LabelTemplatesResponse> {
    let mut templates = super::starters::all();
    templates.extend(
        repository::list_templates(db)
            .await?
            .into_iter()
            .map(LabelTemplateDocument::into_template),
    );
    Ok(LabelTemplatesResponse { templates })
}
