use crate::domain::settings::DesignOption;

/// One published template as the document-server lists it for the calling
/// tenant: the **latest published version** of a design family that is either
/// bundled (global) or granted to the tenant.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    /// The document-server-minted unique key (`tpl_...`) — persisted onto
    /// `generated_documents` rows so the POS side durably holds the ID the
    /// document server assigned. Differs per document-server database.
    pub key: String,
    /// The opaque, immutable template id (`doc_temp_<nanoid>`); one per
    /// version. Identical on every deployment.
    pub name: String,
    /// The design this version belongs to; a shop selects a family and always
    /// prints with its latest version.
    pub family: String,
    pub version: i64,
    /// What the template is for (`a4-invoice`, `thermal-receipt`, …).
    pub kind: String,
    /// Human label (the schema `title`) shown in the dropdowns.
    #[serde(rename = "description")]
    pub title: String,
    /// `document` or `label`.
    pub r#type: String,
    /// `global` (bundled) or `private` (a published package).
    pub scope: String,
    /// The built-in default of its kind (`x-default`).
    pub is_default: bool,
    /// Controls the template declares (`x-options`); their values travel in
    /// the payload's `options` object.
    pub options: Vec<DesignOption>,
    /// JSON Schema for the template's render payload (`dataSchema`) — the one
    /// contract this backend codes its payload builders against.
    pub data_schema: serde_json::Value,
    /// The template's worked sample payload (`data`), used for previews.
    #[serde(rename = "data")]
    pub sample: serde_json::Value,
    pub sha256: String,
    pub min_app_version: Option<String>,
}
