//! The shop's frame size list: one row per size and width (the key is derived from both, see
//! `print_frame_size_key`). A saved row's size and width never change, only whether it is
//! offered. Never deleted: a job that picked one keeps its own copy.

use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::AppError;
use crate::core::error::AppResult;
use crate::domain::print_jobs::CreatePrintFrameSizeRequest;
use crate::domain::print_jobs::PrintFrameSize;
use crate::domain::print_jobs::UpdatePrintFrameSizeRequest;
use crate::domain::print_jobs::frame_width_inches;
use crate::domain::print_jobs::print_frame_size_key;
use crate::modules::print_jobs::model::PrintFrameSizeDocument;
use crate::modules::print_jobs::repository::print_frame_sizes_repository as frame_sizes;
use mongodb::bson::DateTime as BsonDateTime;

const SIZE_MAX: usize = 20;
/// 100 inches: wider than any picture frame moulding.
const WIDTH_MAX_HUNDREDTHS_INCH: i64 = 10_000;

/// The shop's frame sizes; hidden ones only when asked.
pub(crate) async fn list_frame_sizes(
    db: &Db,
    include_hidden: bool,
) -> AppResult<Vec<PrintFrameSize>> {
    Ok(frame_sizes::list(db)
        .await?
        .into_iter()
        .filter(|frame| include_hidden || frame.active)
        .map(PrintFrameSizeDocument::into_print_frame_size)
        .collect())
}

/// A checked new size: trimmed, and size and width within their limits.
fn checked(body: CreatePrintFrameSizeRequest) -> AppResult<CreatePrintFrameSizeRequest> {
    let size = body.size.trim().to_string();
    if size.is_empty() {
        return Err(AppError::validation("Enter the frame size."));
    }
    if size.chars().count() > SIZE_MAX {
        return Err(AppError::validation(format!(
            "A frame size can be at most {SIZE_MAX} characters."
        )));
    }
    if !(1..=WIDTH_MAX_HUNDREDTHS_INCH).contains(&body.width_hundredths_inch) {
        return Err(AppError::validation(
            "A frame width must be between 0.01 and 100 inches.",
        ));
    }
    Ok(CreatePrintFrameSizeRequest {
        size,
        width_hundredths_inch: body.width_hundredths_inch,
    })
}

/// Why a size and width cannot be added again, pointing at a hidden row when that is the one.
fn already_saved(existing: &PrintFrameSizeDocument) -> AppError {
    let frame = format!(
        "{} at {} inch",
        existing.size,
        frame_width_inches(existing.width_hundredths_inch)
    );
    AppError::conflict(
        codes::PRINT_FRAME_SIZE_EXISTS,
        if existing.active {
            format!("{frame} is already in the list.")
        } else {
            format!("{frame} is already saved but hidden. Turn on Show hidden and show it again.")
        },
    )
}

/// Adds a size and width the list does not have yet.
pub(crate) async fn create_frame_size(
    db: &Db,
    body: CreatePrintFrameSizeRequest,
    device_id: Option<String>,
) -> AppResult<PrintFrameSize> {
    crate::core::logging::domain::tracked("print_jobs.frame_size_created", async move {
        let body = checked(body)?;
        let key = print_frame_size_key(&body.size, body.width_hundredths_inch);
        if let Some(existing) = frame_sizes::find(db, &key).await? {
            return Err(already_saved(&existing));
        }
        let now = BsonDateTime::now();
        let document = PrintFrameSizeDocument {
            id: None,
            key,
            size: body.size,
            width_hundredths_inch: body.width_hundredths_inch,
            active: true,
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: device_id,
        };
        Ok(frame_sizes::insert(db, document)
            .await?
            .into_print_frame_size())
    })
    .await
}

/// Hides a saved frame size, or offers it again.
pub(crate) async fn update_frame_size(
    db: &Db,
    key: &str,
    body: UpdatePrintFrameSizeRequest,
    device_id: Option<String>,
) -> AppResult<PrintFrameSize> {
    crate::core::logging::domain::tracked("print_jobs.frame_size_updated", async move {
        let mut document = frame_sizes::find(db, key).await?.ok_or_else(|| {
            AppError::not_found_with_code("Frame size not found", codes::PRINT_FRAME_SIZE_NOT_FOUND)
        })?;
        let expected = document.version;
        document.active = body.active;
        document.version = expected + 1;
        document.updated_at = BsonDateTime::now();
        document.updated_by_device = device_id;
        if !frame_sizes::replace(db, &document, expected).await? {
            return Err(AppError::conflict(
                codes::VERSION_CONFLICT,
                "Someone else changed this frame size just now. Try again.",
            ));
        }
        Ok(document.into_print_frame_size())
    })
    .await
}

/// Converts a page of raw `print_frame_sizes` documents for the sync feed.
pub(crate) fn hydrate_sync_frame_sizes(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<PrintFrameSize>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<PrintFrameSizeDocument>(document)?
                    .into_print_frame_size(),
            )
        })
        .collect()
}
