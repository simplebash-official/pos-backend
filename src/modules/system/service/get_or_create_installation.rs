use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::core::id::generate_id;
use crate::domain::system::SystemInstallation;
use crate::modules::system::repository;
use chrono::Utc;
use std::env;

/// Retrieves the existing installation record, or creates and persists a new one.
pub(crate) async fn get_or_create_installation(db: &Db) -> AppResult<SystemInstallation> {
    if let Some(existing) = repository::get_installation(db).await? {
        return Ok(existing);
    }

    let installation_id =
        env::var("INSTALLATION_ID").unwrap_or_else(|_| format!("inst_{}", nanoid::nanoid!(16)));
    let app_version = env::var("APP_VERSION").unwrap_or_else(|_| "0.5.0".to_string());
    let platform = env::var("PLATFORM").unwrap_or_else(|_| std::env::consts::OS.to_string());
    let now = Utc::now().to_rfc3339();

    let new_inst = SystemInstallation {
        key: generate_id("inst"),
        id: generate_id("inst"),
        installation_id,
        app_version,
        platform,
        installed_at: now.clone(),
        setup_completed: false,
        setup_completed_at: None,
        sample_data_loaded: false,
        created_at: now.clone(),
        updated_at: now,
    };

    repository::create_installation(db, &new_inst).await?;
    Ok(new_inst)
}
