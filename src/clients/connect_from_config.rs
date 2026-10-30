use super::db::Db;
use super::tenant_db::TenantDatabase;
use crate::core::config::{Config, DatabaseType};
use crate::core::error::AppError;

/// Connects to the database engine specified by `Config`.
pub async fn connect_from_config(config: &Config) -> Result<Db, AppError> {
    match config.database_type {
        DatabaseType::Sqlite => {
            tracing::info!(url = %config.database_url, "Connecting to SQLite database...");
            let pool = super::sqlite::connect(&config.database_url)
                .await
                .map_err(|e| AppError::internal(format!("failed to connect to SQLite: {e}")))?;
            tracing::info!("Successfully connected to SQLite database");
            Ok(Db::Sqlite(pool))
        }
        DatabaseType::Mongo => {
            tracing::info!(
                db = %config.mongodb_db_name,
                "Connecting to MongoDB database..."
            );
            let mongo_db = super::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
                .await
                .map_err(|e| AppError::internal(format!("failed to connect to MongoDB: {e}")))?;
            tracing::info!(db = %config.mongodb_db_name, "Successfully connected to MongoDB");
            Ok(match config.tenant_mode {
                crate::core::config::TenantMode::Multi => {
                    Db::Mongo(TenantDatabase::multi_tenant(mongo_db))
                }
                crate::core::config::TenantMode::Single => Db::from_mongo(mongo_db),
            })
        }
    }
}
