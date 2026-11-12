//! The unified database handle (`Db`) over MongoDB and SQLite.

use super::tenant_db::TenantDatabase;

use crate::core::error::{AppError, AppResult};
use crate::core::tenancy::Tenant;
use mongodb::Database as MongoDatabase;
use sqlx::SqlitePool;

/// Unified database handle wrapped into AppState.
/// Both `mongodb::Database` and `sqlx::SqlitePool` are internally reference-counted
/// handles (`Arc`), making `Db` lightweight to clone across requests.
#[derive(Debug, Clone)]
pub enum Db {
    Mongo(TenantDatabase),
    Sqlite(SqlitePool),
}

impl Db {
    /// True when configured with MongoDB.
    #[must_use]
    pub fn is_mongo(&self) -> bool {
        matches!(self, Db::Mongo(_))
    }

    /// True when configured with SQLite.
    #[must_use]
    pub fn is_sqlite(&self) -> bool {
        matches!(self, Db::Sqlite(_))
    }

    /// Wraps a raw driver handle as an unscoped (single-tenant) `Db`.
    #[must_use]
    pub fn from_mongo(db: MongoDatabase) -> Db {
        Db::Mongo(TenantDatabase::global(db))
    }

    /// The same database confined to one tenant. SQLite is always single-tenant
    /// (the desktop app), so it is returned unchanged.
    #[must_use]
    pub fn for_tenant(&self, tenant: Tenant) -> Db {
        match self {
            Db::Mongo(db) => Db::Mongo(db.for_tenant(tenant)),
            Db::Sqlite(pool) => Db::Sqlite(pool.clone()),
        }
    }

    /// Returns a reference to the tenant-aware Mongo handle if running on Mongo.
    #[must_use]
    pub fn as_mongo(&self) -> Option<&TenantDatabase> {
        match self {
            Db::Mongo(db) => Some(db),
            _ => None,
        }
    }

    /// Returns a reference to the inner `mongodb::Database`, panicking if running on SQLite.
    #[must_use]
    pub fn mongo(&self) -> &TenantDatabase {
        self.as_mongo()
            .expect("Attempted to access MongoDB handle when running in SQLite mode")
    }

    /// Returns a reference to the inner `sqlx::SqlitePool` if running on SQLite.
    #[must_use]
    pub fn as_sqlite(&self) -> Option<&SqlitePool> {
        match self {
            Db::Sqlite(pool) => Some(pool),
            _ => None,
        }
    }

    /// Returns a reference to the inner `sqlx::SqlitePool`, panicking if running on Mongo.
    #[must_use]
    pub fn sqlite(&self) -> &SqlitePool {
        self.as_sqlite()
            .expect("Attempted to access SQLite pool when running in MongoDB mode")
    }

    /// Returns the database engine name for logging and diagnostics.
    #[must_use]
    pub fn engine_name(&self) -> &'static str {
        match self {
            Db::Mongo(_) => "mongodb",
            Db::Sqlite(_) => "sqlite",
        }
    }

    /// Rejects operations executed against a multi-tenant MongoDB handle with no
    /// active tenant in scope (`Tenant::Deny`) so seeding/backfill cannot stamp
    /// `"__deny__"` rows into tenant-scoped collections.
    pub fn require_active_tenant_scope(&self) -> AppResult<()> {
        if let Some(mongo) = self.as_mongo()
            && matches!(mongo.effective_tenant(), Tenant::Deny)
        {
            return Err(AppError::internal(
                "Cannot write shop-scoped data without an active tenant scope",
            ));
        }
        Ok(())
    }
}
