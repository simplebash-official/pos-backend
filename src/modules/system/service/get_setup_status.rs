use super::get_or_create_installation::get_or_create_installation;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::system::SetupStatusResponse;
use crate::modules::system::repository;

/// Evaluates whether initial system setup has been completed or is required.
pub(crate) async fn get_setup_status(db: &Db) -> AppResult<SetupStatusResponse> {
    let installation = get_or_create_installation(db).await?;
    let users_count = repository::count_users(db).await?;

    // Invariant: setup is truly completed only if marked in installation AND at least one user exists.
    let setup_completed = installation.setup_completed && users_count > 0;
    let is_first_run = !setup_completed && users_count == 0;

    Ok(SetupStatusResponse {
        setup_completed,
        is_first_run,
        installation_id: Some(installation.installation_id),
        installed_at: Some(installation.installed_at),
        setup_completed_at: installation.setup_completed_at,
        sample_data_loaded: Some(installation.sample_data_loaded),
        app_version: installation.app_version,
        platform: installation.platform,
    })
}
