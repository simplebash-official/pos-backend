use super::ensure_name_free::ensure_name_free;

use crate::clients::db::Db;

use crate::core::constants::prefixes;
use crate::core::error::AppResult;
use crate::core::id::generate_id;
use crate::domain::labels::{LabelTemplate, LabelTemplateInput};
use crate::modules::labels::model::LabelTemplateDocument;
use crate::modules::labels::repository;

use mongodb::bson::DateTime as BsonDateTime;

/// Saves a new shop design.
pub(crate) async fn create_template(
    db: &Db,
    input: LabelTemplateInput,
) -> AppResult<LabelTemplate> {
    crate::core::logging::domain::tracked("labels.template_created", async move {
        super::validate::validate_template(&input)?;
        ensure_name_free(db, &input.name, None).await?;
        let now = BsonDateTime::now();
        let document = LabelTemplateDocument {
            id: None,
            key: generate_id(prefixes::LABEL_TEMPLATE),
            name: input.name.trim().to_string(),
            width_mm: input.width_mm,
            height_mm: input.height_mm,
            elements: input.elements,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        repository::insert_template(db, &document).await?;
        Ok(document.into_template())
    })
    .await
}
