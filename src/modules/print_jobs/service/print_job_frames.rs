//! The frames a customer wants on a photo and framing job. They say what to make and change no
//! money: a job is priced by its items.

use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::AppError;
use crate::core::error::AppResult;
use crate::domain::print_jobs::PrintJobFrame;
use crate::domain::print_jobs::PrintJobFrameInput;
use crate::domain::print_jobs::frame_width_inches;
use crate::modules::print_jobs::repository::print_frame_sizes_repository as frame_sizes;
use pos_calc_core::print_job::MAX_LINE_QUANTITY;
use std::collections::HashSet;

/// The most different frames one job lists.
const MAX_JOB_FRAMES: usize = 50;
/// The only kind of order that lists frames.
const FRAMING_JOB_TYPE: &str = "photo-framing";

/// Checks the frames the print form sent, in order. A frame already on the job keeps the size and
/// width it was saved with, even if it has since been hidden; a newly picked one must be offered
/// by the shop's list, which says its size and width.
pub(super) async fn check_frames(
    db: &Db,
    inputs: Vec<PrintJobFrameInput>,
    existing: &[PrintJobFrame],
) -> AppResult<Vec<PrintJobFrame>> {
    if inputs.len() > MAX_JOB_FRAMES {
        return Err(AppError::validation(format!(
            "A job can list at most {MAX_JOB_FRAMES} frames."
        )));
    }
    let mut seen = HashSet::new();
    let mut frames = Vec::with_capacity(inputs.len());
    for input in inputs {
        if !seen.insert(input.frame_size_key.clone()) {
            return Err(AppError::validation(
                "A frame is listed twice; change its quantity instead.",
            ));
        }
        if !(1..=MAX_LINE_QUANTITY).contains(&input.quantity) {
            return Err(AppError::validation(format!(
                "A frame quantity must be between 1 and {MAX_LINE_QUANTITY}."
            )));
        }
        let saved = existing
            .iter()
            .find(|frame| frame.frame_size_key == input.frame_size_key);
        let frame = match saved {
            Some(frame) => PrintJobFrame {
                quantity: input.quantity,
                ..frame.clone()
            },
            None => picked_frame(db, input).await?,
        };
        frames.push(frame);
    }
    Ok(frames)
}

/// A newly picked frame, read from the shop's list.
async fn picked_frame(db: &Db, input: PrintJobFrameInput) -> AppResult<PrintJobFrame> {
    let listed = frame_sizes::find(db, &input.frame_size_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Frame size not found", codes::PRINT_FRAME_SIZE_NOT_FOUND)
        })?;
    if !listed.active {
        return Err(AppError::validation(format!(
            "{} at {} inch is hidden, so it cannot be added to a job.",
            listed.size,
            frame_width_inches(listed.width_hundredths_inch)
        )));
    }
    Ok(PrintJobFrame {
        frame_size_key: listed.key,
        size: listed.size,
        width_hundredths_inch: listed.width_hundredths_inch,
        quantity: input.quantity,
    })
}

/// Refuses frames on any other kind of order.
pub(super) fn ensure_framing_job(job_type: &str, frames: &[PrintJobFrame]) -> AppResult<()> {
    if job_type != FRAMING_JOB_TYPE && !frames.is_empty() {
        return Err(AppError::validation(
            "Only a Photo & Framing job lists frames. Remove them to change the type.",
        ));
    }
    Ok(())
}
