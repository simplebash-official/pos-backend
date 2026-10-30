use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::sessions::repository::movements;

/// Which of these movement keys have been reversed.
pub(crate) async fn reversed_keys(
    db: &Db,
    keys: &[String],
) -> AppResult<std::collections::HashSet<String>> {
    Ok(movements::find_reversals_of(db, keys)
        .await?
        .into_iter()
        .map(|(original, _)| original)
        .collect())
}
