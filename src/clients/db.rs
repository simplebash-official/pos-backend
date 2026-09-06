// Unified database handle abstraction supporting both MongoDB and SQLite.

use mongodb::Database as MongoDatabase;
use sqlx::SqlitePool;

use crate::core::{
    config::{Config, DatabaseType},
    error::AppError,
};

/// Unified database handle wrapped into AppState.
/// Both `mongodb::Database` and `sqlx::SqlitePool` are internally reference-counted
/// handles (`Arc`), making `Db` lightweight to clone across requests.
#[derive(Clone)]
pub enum Db {
    Mongo(MongoDatabase),
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

    /// Returns a reference to the inner `mongodb::Database` if running on Mongo.
    pub fn as_mongo(&self) -> Option<&MongoDatabase> {
        match self {
            Db::Mongo(db) => Some(db),
            _ => None,
        }
    }

    /// Returns a reference to the inner `mongodb::Database`, panicking if running on SQLite.
    pub fn mongo(&self) -> &MongoDatabase {
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
            Ok(Db::Mongo(mongo_db))
        }
    }
}
