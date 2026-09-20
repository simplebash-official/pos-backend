// Unified database handle abstraction supporting both MongoDB and SQLite.

use mongodb::Database as MongoDatabase;
use sqlx::SqlitePool;

use super::tenant_db::TenantDatabase;
use crate::core::tenancy::Tenant;

use crate::core::{
    config::{Config, DatabaseType},
    error::AppError,
};

/// Unified database handle wrapped into AppState.
/// Both `mongodb::Database` and `sqlx::SqlitePool` are internally reference-counted
/// handles (`Arc`), making `Db` lightweight to clone across requests.
#[derive(Clone)]
pub enum Db {
    Mongo(TenantDatabase),
    Sqlite(SqlitePool),
}

impl Db {
    /// True when configured with MongoDB.
    pub fn is_mongo(&self) -> bool {
        matches!(self, Db::Mongo(_))
    }

    /// True when configured with SQLite.
    pub fn is_sqlite(&self) -> bool {
        matches!(self, Db::Sqlite(_))
    }

    /// Wraps a raw driver handle as an unscoped (single-tenant) `Db`.
    pub fn from_mongo(db: MongoDatabase) -> Db {
        Db::Mongo(TenantDatabase::global(db))
    }

    /// The same database confined to one tenant. SQLite is always single-tenant
    /// (the desktop app), so it is returned unchanged.
    pub fn for_tenant(&self, tenant: Tenant) -> Db {
        match self {
            Db::Mongo(db) => Db::Mongo(db.for_tenant(tenant)),
            Db::Sqlite(pool) => Db::Sqlite(pool.clone()),
        }
    }

    /// Returns a reference to the tenant-aware Mongo handle if running on Mongo.
    pub fn as_mongo(&self) -> Option<&TenantDatabase> {
        match self {
            Db::Mongo(db) => Some(db),
            _ => None,
        }
    }

    /// Returns a reference to the inner `mongodb::Database`, panicking if running on SQLite.
    pub fn mongo(&self) -> &TenantDatabase {
        self.as_mongo()
            .expect("Attempted to access MongoDB handle when running in SQLite mode")
    }

    /// Returns a reference to the inner `sqlx::SqlitePool` if running on SQLite.
    pub fn as_sqlite(&self) -> Option<&SqlitePool> {
        match self {
            Db::Sqlite(pool) => Some(pool),
            _ => None,
        }
    }

    /// Returns a reference to the inner `sqlx::SqlitePool`, panicking if running on Mongo.
    pub fn sqlite(&self) -> &SqlitePool {
        self.as_sqlite()
            .expect("Attempted to access SQLite pool when running in MongoDB mode")
    }

    /// Returns the database engine name for logging and diagnostics.
    pub fn engine_name(&self) -> &'static str {
        match self {
            Db::Mongo(_) => "mongodb",
            Db::Sqlite(_) => "sqlite",
        }
    }
}

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
