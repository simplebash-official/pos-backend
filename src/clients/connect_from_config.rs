use super::db::Db;
use super::tenant_db::TenantDatabase;
use crate::core::config::{Config, Database, TenantMode};
use crate::core::error::AppError;

/// Connects to the database engine specified by `Config`.
pub async fn connect_from_config(config: &Config) -> Result<Db, AppError> {
    match &config.database {
        Database::Sqlite { url } => {
            tracing::info!(url = %url, "Connecting to SQLite database...");
            let pool = super::sqlite::connect(url, config.logging.sql)
                .await
                .map_err(|e| AppError::internal(format!("failed to connect to SQLite: {e}")))?;
            tracing::info!("Successfully connected to SQLite database");
            Ok(Db::Sqlite(pool))
        }
        Database::Mongo { uri, name } => {
            tracing::info!(db = %name, "Connecting to MongoDB database...");
            let mongo_db = super::mongo::connect(uri, name)
                .await
                .map_err(|e| AppError::internal(format!("failed to connect to MongoDB: {e}")))?;
            tracing::info!(db = %name, "Successfully connected to MongoDB");
            Ok(match config.tenant_mode {
                TenantMode::Multi => Db::Mongo(TenantDatabase::multi_tenant(mongo_db)),
                TenantMode::Single => Db::from_mongo(mongo_db),
            })
        }
    }
}
