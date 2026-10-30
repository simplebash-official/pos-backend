use super::validate_job_request::validate_job_request;
use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;
use crate::core::constants::prefixes;
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;

use crate::domain::labels::{CreateLabelJobRequest, LabelJob, LabelJobOutcome, LabelPrintAction};

use crate::modules::labels::model::{LabelJobItemDocument, LabelPrintJobDocument};
use crate::modules::labels::repository;

use mongodb::bson::DateTime as BsonDateTime;

/// Renders the chosen items like `render_labels` and records the job. Nothing is
/// recorded when the items cannot be printed. Returns the job and the PDF.
pub(crate) async fn create_job(
    db: &Db,
    doc_server: &DocumentServerClient,
    tenant_id: Option<&str>,
    printed_by_user_id: &str,
    printed_by_name: &str,
    request: CreateLabelJobRequest,
) -> AppResult<(LabelJob, Vec<u8>)> {
    validate_job_request(&request)?;
    let template = super::super::find_template(db, &request.template_key).await?;
    let stickers = super::super::resolve_stickers(
        db,
        super::super::has_price_element(&template.elements),
        &request.items,
    )
    .await?;
    let pdf = super::super::render(
        db,
        doc_server,
        tenant_id,
        template.width_mm,
        template.height_mm,
        &template.elements,
        &stickers,
    )
    .await?;

    let job = crate::core::logging::domain::tracked("labels.job_created", async {
        let now = BsonDateTime::now();
        let document = LabelPrintJobDocument {
            id: None,
            key: generate_id(prefixes::LABEL_PRINT_JOB),
            template_key: template.key.clone(),
            template_name: template.name.clone(),
            width_mm: template.width_mm,
            height_mm: template.height_mm,
            action: request.action,
            method: request.method,
            printer_name: request
                .printer_name
                .as_deref()
                .map(|n| n.trim().to_string()),
            dpi: request.dpi,
            // A downloaded file is handed over at once; a print waits for the computer's report.
            outcome: match request.action {
                LabelPrintAction::Download => LabelJobOutcome::Sent,
                LabelPrintAction::Print => LabelJobOutcome::Rendered,
            },
            error: None,
            printed_by_user_id: printed_by_user_id.to_string(),
            printed_by_name: printed_by_name.to_string(),
            items: stickers
                .iter()
                .map(|sticker| LabelJobItemDocument {
                    product_key: sticker.product_key.clone(),
                    product_name: sticker.name.clone(),
                    sku: sticker.sku.clone(),
                    serial_number: sticker.serial_number.clone(),
                    price_cents: sticker.price_cents,
                    copies: sticker.copies,
                })
                .collect(),
            total_labels: stickers.iter().map(|sticker| sticker.copies).sum(),
            created_at: now,
            updated_at: now,
        };
        repository::insert_job(db, &document).await?;
        Ok::<_, AppError>(document.into_job())
    })
    .await?;
    Ok((job, pdf))
}
