use super::design_settings::DesignSettings;
use super::merge_settings_in::merge_settings_in;
use super::option_values::OptionValues;
use crate::clients::document_server::DocumentServerClient;
use crate::core::error::AppResult;

use std::collections::HashMap;

pub(crate) async fn merge_settings(
    client: &DocumentServerClient,
    current: DesignSettings,
    selection_update: &HashMap<String, Option<String>>,
    option_update: &OptionValues,
) -> AppResult<DesignSettings> {
    let templates = client.list_templates().await?;
    merge_settings_in(&templates, current, selection_update, option_update)
}
