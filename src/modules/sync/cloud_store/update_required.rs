use crate::core::error::AppError;

/// Refusal for a desktop older than [`MIN_DESKTOP_APP_VERSION`]. A 503 on
/// purpose: older apps retry a 5xx later and keep their waiting changes,
/// whereas any other 4xx makes them drop the refused change for good.
pub(super) fn update_required() -> AppError {
    AppError::custom(
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "APP_UPDATE_REQUIRED",
        "This SimpleBash app is too old to sync. Update it under Settings > Updates. \
         Everything you do on this computer is kept and syncs after the update.",
    )
}
