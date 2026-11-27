//! Pure types for the slip a customer gets for the repair and print jobs they leave with the
//! shop. The printing itself lives in `modules::job_receipts`; the payload builder in
//! `modules::billing::service::print_payload`.

mod job_receipt;

pub use job_receipt::JobReceipt;
pub use job_receipt::JobReceiptCustomer;
pub use job_receipt::JobReceiptJob;
pub use job_receipt::JobReceiptKey;
pub use job_receipt::JobReceiptKind;
pub use job_receipt::JobReceiptPaper;
pub use job_receipt::JobReceiptQuery;
pub use job_receipt::JobReceiptSource;
pub use job_receipt::MAX_RECEIPT_JOBS;
