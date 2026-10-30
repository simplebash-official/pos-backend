use crate::core::error::AppResult;

use sqlx::SqlitePool;

/// A small stable numeric tag for this device (0..999), derived from its id.
/// Used only for barcode's fallback series, where the tag must be numeric
/// digits within an EAN-13 payload rather than free text — see
/// `next_raw_number`.
pub(crate) async fn device_numeric_tag(pool: &SqlitePool) -> AppResult<i64> {
    let device_id: String = sqlx::query_scalar("SELECT device_id FROM sync_state WHERE id = 1")
        .fetch_optional(pool)
        .await?
        .unwrap_or_default();
    Ok(device_id
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32)) as i64
        % 1000)
}
