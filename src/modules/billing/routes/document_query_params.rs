use crate::domain::settings::DocumentDesignChoice;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct DocumentQueryParams {
    #[serde(rename = "paperWidthMm")]
    pub(super) paper_width_mm: Option<u32>,
    /// A specific design family for this one print. Omitted → the shop's
    /// chosen design for the document kind.
    pub(super) template: Option<String>,
    /// `original` serves the PDF exactly as first printed; `current` prints
    /// with the design in force now. Required: the caller decides.
    pub(super) design: DocumentDesignChoice,
}
