//! Storage document shapes for a shop's own label designs and its label print
//! history. Kept apart from `domain::labels`, which holds the API-facing types.

mod labels_model;

pub use labels_model::LabelJobItemDocument;
pub use labels_model::LabelPrintJobDocument;
pub use labels_model::LabelTemplateDocument;
