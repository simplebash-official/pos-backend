use super::checksum_digit::checksum_digit;
use crate::core::error::{AppError, AppResult};

/// Maps a barcode namespace (a caller-owned string, e.g. `"product"`) to
/// its fixed 2-digit reserved prefix. An explicit per-namespace registry
/// rather than a hash — so two namespaces can never be assigned the same
/// prefix, which combined with each namespace's counter independently
/// starting at 1 would otherwise let two different namespaces mint the same
/// barcode. Add a new arm here when a new label-worthy entity needs one.
fn prefix_for_namespace(namespace: &str) -> AppResult<&'static str> {
    match namespace {
        "product" => Ok("20"),
        other => Err(AppError::internal(format!(
            "no reserved EAN-13 prefix registered for barcode namespace '{other}'"
        ))),
    }
}

/// Encodes `sequence` under `namespace`'s reserved prefix into a raw
/// 13-digit numeric string (no dashes/spacing — display formatting, if
/// ever needed, happens at render/print time, never here). Errors if
/// `sequence` doesn't fit in the 10-digit sequence field, i.e. the
/// namespace's counter space is exhausted — expected to never happen in
/// practice.
pub(crate) fn encode(namespace: &str, sequence: i64) -> AppResult<String> {
    let prefix = prefix_for_namespace(namespace)?;

    if !(0..=9_999_999_999).contains(&sequence) {
        return Err(AppError::internal(format!(
            "barcode sequence {sequence} out of range for namespace '{namespace}'"
        )));
    }

    let twelve = format!("{prefix}{sequence:010}");
    let check = checksum_digit(&twelve);
    Ok(format!("{twelve}{check}"))
}
