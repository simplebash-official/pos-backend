use super::design_settings::DesignSettings;
use super::resolve_in::resolve_in;
use super::resolved_template::ResolvedTemplate;
use crate::clients::document_server::DocumentServerClient;
use crate::core::error::AppResult;

pub(crate) async fn resolve(
    client: &DocumentServerClient,
    kind: &str,
    requested: Option<&str>,
    settings: &DesignSettings,
) -> AppResult<ResolvedTemplate> {
    let templates = client.list_templates().await?;
    resolve_in(&templates, kind, requested, settings)
}
