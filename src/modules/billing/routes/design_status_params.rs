use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct DesignStatusParams {
    #[serde(rename = "paperWidthMm")]
    pub(super) paper_width_mm: Option<u32>,
    pub(super) template: Option<String>,
}
