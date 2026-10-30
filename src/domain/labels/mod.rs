// Pure business types for label printing: a shop-designed label layout and the
// requests that print it. No I/O, no Mongo/Axum types beyond serde/utoipa
// derives. The layout is plain data (absolutely placed elements, in
// millimetres); document-server's "Product Sticker Label" template draws it.

mod create_label_job_request;
mod label_coverage_counts;
mod label_coverage_query;
mod label_coverage_response;
mod label_coverage_row;
mod label_coverage_status;
mod label_element;
mod label_frame;
mod label_item;
mod label_job;
mod label_job_item;
mod label_job_list_query;
mod label_job_list_response;
mod label_job_outcome;
mod label_print_action;
mod label_print_method;
mod label_sample;
mod label_template;
mod label_template_input;
mod label_templates_response;
mod preview_labels_request;
mod render_labels_request;
mod text_align;
mod text_source;
mod update_label_job_request;

pub use create_label_job_request::CreateLabelJobRequest;
pub use label_coverage_counts::LabelCoverageCounts;
pub use label_coverage_query::LabelCoverageQuery;
pub use label_coverage_response::LabelCoverageResponse;
pub use label_coverage_row::LabelCoverageRow;
pub use label_coverage_status::LabelCoverageStatus;
pub use label_element::LabelElement;
pub use label_frame::LabelFrame;
pub use label_item::LabelItem;
pub use label_job::LabelJob;
pub use label_job_item::LabelJobItem;
pub use label_job_list_query::LabelJobListQuery;
pub use label_job_list_response::LabelJobListResponse;
pub use label_job_outcome::LabelJobOutcome;
pub use label_print_action::LabelPrintAction;
pub use label_print_method::LabelPrintMethod;
pub use label_sample::LabelSample;
pub use label_template::LabelTemplate;
pub use label_template_input::LabelTemplateInput;
pub use label_templates_response::LabelTemplatesResponse;
pub use preview_labels_request::PreviewLabelsRequest;
pub use render_labels_request::RenderLabelsRequest;
pub use text_align::TextAlign;
pub use text_source::TextSource;
pub use update_label_job_request::UpdateLabelJobRequest;
