use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What the person did with the stickers: sent them to a printer or saved the PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LabelPrintAction {
    Print,
    Download,
}
