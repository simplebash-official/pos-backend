use super::get_or_create_installation::get_or_create_installation;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::system::SystemInstallation;

/// Retrieves the complete installation metadata record for system inspection.
pub(crate) async fn get_installation_info(db: &Db) -> AppResult<SystemInstallation> {
    get_or_create_installation(db).await
}
