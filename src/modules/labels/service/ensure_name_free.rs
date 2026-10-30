use super::list_templates::list_templates;

use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use axum::http::StatusCode;

const TEMPLATE_NAME_TAKEN: &str = "LABEL_TEMPLATE_NAME_TAKEN";

/// Design names are unique per shop (starters included), ignoring case, so the
/// template picker never shows two identical entries.
pub(super) async fn ensure_name_free(db: &Db, name: &str, own_key: Option<&str>) -> AppResult<()> {
    let wanted = name.trim().to_lowercase();
    let taken = list_templates(db)
        .await?
        .templates
        .iter()
        .any(|t| Some(t.key.as_str()) != own_key && t.name.trim().to_lowercase() == wanted);
    if taken {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            TEMPLATE_NAME_TAKEN,
            format!("A label design called '{}' already exists", name.trim()),
        ));
    }
    Ok(())
}
