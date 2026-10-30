use super::design_settings::DesignSettings;
use super::list_in::list_in;
use crate::clients::document_server::DocumentServerClient;
use crate::core::error::AppResult;
use crate::domain::settings::DocumentKindSettings;

pub(crate) async fn list(
    client: &DocumentServerClient,
    settings: &DesignSettings,
) -> AppResult<Vec<DocumentKindSettings>> {
    let templates = client.list_templates().await?;
    list_in(&templates, settings)
}
