use super::label_item::LabelItem;
use super::label_print_action::LabelPrintAction;
use super::label_print_method::LabelPrintMethod;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Body of `POST /labels/jobs`: what to print plus how it leaves the computer.
/// `method` and `dpi` are required for a print and must be absent for a
/// download; `printer_name` is required exactly when `method` is `direct`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateLabelJobRequest {
    pub template_key: String,
    pub items: Vec<LabelItem>,
    pub action: LabelPrintAction,
    pub method: Option<LabelPrintMethod>,
    pub printer_name: Option<String>,
    pub dpi: Option<u32>,
}
