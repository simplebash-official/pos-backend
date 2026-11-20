use crate::app::AppState;
use crate::core::error::{AppError, AppResult};
use crate::domain::job_receipts::{JobReceiptPaper, JobReceiptSource};
use crate::domain::settings::DocumentDesignChoice;
use crate::modules::billing;
use crate::modules::documents;
use crate::modules::settings;
use sha2::{Digest, Sha256};

/// The document kind a job receipt is printed as.
const KIND: &str = "job-receipt";

/// The PDF of a job receipt with the design in force now. A receipt always shows the job and
/// the shop's details as they are today, so it is cached under a digest of the whole payload:
/// an edited ticket, edited terms or a changed design each print afresh, and there is no
/// "original" to reprint.
///
/// # Errors
///
/// A 422 when the requested or chosen design is not available, an internal error when the
/// shop profile is missing or the ticket holds a value that cannot be printed.
pub(crate) async fn print(
    state: &AppState,
    source: &JobReceiptSource,
    paper: JobReceiptPaper,
    template: Option<&str>,
) -> AppResult<Vec<u8>> {
    let shop = settings::service::get_shop_profile(&state.db).await?;
    let data = billing::service::print_payload::build_job_receipt_data(source, &shop, paper)?;
    let serialized = serde_json::to_vec(&data)
        .map_err(|err| AppError::internal(format!("job receipt payload is not JSON: {err}")))?;
    let digest: String = Sha256::digest(&serialized)
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let cache_key = format!("{KIND}-{}-v{}-{digest}", paper.as_str(), source.version);
    documents::service::print(
        &state.db,
        &state.document_server,
        &state.config.generated_documents_dir,
        KIND,
        template,
        DocumentDesignChoice::Current,
        &source.key,
        &cache_key,
        data,
    )
    .await
}
