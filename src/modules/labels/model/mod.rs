// Storage document shapes for a shop's own label designs and its label print
// history. Kept apart from `domain::labels`, which holds the API-facing types.

mod label_job_item_document;
mod label_print_job_document;
mod label_template_document;

pub use label_job_item_document::LabelJobItemDocument;
pub use label_print_job_document::LabelPrintJobDocument;
pub use label_template_document::LabelTemplateDocument;
