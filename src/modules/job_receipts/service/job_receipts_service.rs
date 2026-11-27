use crate::app::AppState;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::job_receipts::{
    JobReceipt, JobReceiptKey, JobReceiptKind, JobReceiptPaper, JobReceiptSource,
};
use crate::domain::settings::DocumentDesignChoice;
use crate::modules::{billing, documents, print_jobs, repairs, settings};
use sha2::{Digest, Sha256};

/// The document kind a job receipt is printed as.
const KIND: &str = "job-receipt";

/// Reads the jobs asked for and puts them on one slip.
///
/// # Errors
///
/// A 404 for an unknown or deleted job, a 400 when the jobs are not one customer's, and an
/// internal error when a ticket holds a value that cannot be printed.
pub(crate) async fn load(db: &Db, keys: &[JobReceiptKey]) -> AppResult<JobReceipt> {
    let mut sources = Vec::with_capacity(keys.len());
    for job in keys {
        sources.push(match job.kind {
            JobReceiptKind::Repair => {
                let repair = repairs::service::get_repair(db, &job.key).await?;
                JobReceiptSource::from_repair(&repair)
            }
            JobReceiptKind::Print => {
                let print_job = print_jobs::service::get_print_job(db, &job.key).await?;
                JobReceiptSource::from_print_job(&print_job)?
            }
        });
    }
    JobReceipt::new(sources)
}

/// The PDF of a slip with the design in force now. A slip always shows the jobs and the shop's
/// details as they are today, so it is cached under the customer and a digest of the whole
/// payload: other jobs, an edited ticket, edited terms or a changed design each print afresh,
/// and there is no "original" to reprint.
///
/// # Errors
///
/// A 422 when the requested or chosen design is not available, an internal error when the
/// shop profile is missing or a ticket holds a value that cannot be printed.
pub(crate) async fn print(
    state: &AppState,
    receipt: &JobReceipt,
    paper: JobReceiptPaper,
    template: Option<&str>,
) -> AppResult<Vec<u8>> {
    let shop = settings::service::get_shop_profile(&state.db).await?;
    let data = billing::service::print_payload::build_job_receipt_data(receipt, &shop, paper)?;
    let serialized = serde_json::to_vec(&data)
        .map_err(|err| AppError::internal(format!("job receipt payload is not JSON: {err}")))?;
    let digest: String = Sha256::digest(&serialized)
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let cache_key = format!("{KIND}-{}-{digest}", paper.as_str());
    documents::service::print(
        &state.db,
        &state.document_server,
        &state.config.generated_documents_dir,
        KIND,
        template,
        DocumentDesignChoice::Current,
        &receipt.customer.key,
        &cache_key,
        data,
    )
    .await
}
