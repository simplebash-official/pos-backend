//! Pure types for the receipt a customer gets when the shop receives a repair or a print job.
//! The printing itself lives in `modules::job_receipts`; the payload builder in
//! `modules::billing::service::print_payload`.

mod job_receipt;

pub use job_receipt::JobReceiptKind;
pub use job_receipt::JobReceiptPaper;
pub use job_receipt::JobReceiptQuery;
pub use job_receipt::JobReceiptSource;
