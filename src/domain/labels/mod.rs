//! Pure business types for label printing: a shop-designed label layout and the
//! requests that print it. No I/O, no Mongo/Axum types beyond serde/utoipa
//! derives. The layout is plain data (absolutely placed elements, in
//! millimetres); document-server's "Product Sticker Label" template draws it.

mod domain_labels;

pub use domain_labels::CreateLabelJobRequest;
pub use domain_labels::LabelCoverageCounts;
pub use domain_labels::LabelCoverageQuery;
pub use domain_labels::LabelCoverageResponse;
pub use domain_labels::LabelCoverageRow;
pub use domain_labels::LabelCoverageStatus;
pub use domain_labels::LabelElement;
pub use domain_labels::LabelFrame;
pub use domain_labels::LabelItem;
pub use domain_labels::LabelJob;
pub use domain_labels::LabelJobItem;
pub use domain_labels::LabelJobListQuery;
pub use domain_labels::LabelJobListResponse;
pub use domain_labels::LabelJobOutcome;
pub use domain_labels::LabelPrintAction;
pub use domain_labels::LabelPrintMethod;
pub use domain_labels::LabelSample;
pub use domain_labels::LabelTemplate;
pub use domain_labels::LabelTemplateInput;
pub use domain_labels::LabelTemplatesResponse;
pub use domain_labels::PreviewLabelsRequest;
pub use domain_labels::RenderLabelsRequest;
pub use domain_labels::TextAlign;
pub use domain_labels::TextSource;
pub use domain_labels::UpdateLabelJobRequest;
