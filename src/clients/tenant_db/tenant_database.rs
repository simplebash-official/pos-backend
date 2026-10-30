use super::scoped_collection::ScopedCollection;

use crate::core::sync_origin::{CLOUD_ORIGIN, current_origin, is_synced_collection};
use crate::core::tenancy::{Tenant, current_tenant};

use mongodb::Database;

use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TenantDatabase {
    pub(super) inner: Database,
    /// Multi-tenant handles resolve the tenant when a collection is opened;
    /// single-tenant ones never filter.
    pub(super) multi_tenant: bool,
    /// Set by `for_tenant`; wins over the ambient request tenant.
    pub(super) pinned: Option<Tenant>,
}

impl TenantDatabase {
    /// Unscoped handle (single-shop deployments and platform collections).
    #[must_use]
    pub fn global(inner: Database) -> Self {
        Self {
            inner,
            multi_tenant: false,
            pinned: None,
        }
    }

    /// Multi-tenant handle: each collection opened is confined to the ambient
    /// tenant of the running request (`core::tenancy::with_tenant`), or to
    /// `Tenant::Deny` (no data) when there is none.
    #[must_use]
    pub fn multi_tenant(inner: Database) -> Self {
        Self {
            inner,
            multi_tenant: true,
            pinned: None,
        }
    }

    /// Same database, permanently confined to one tenant.
    #[must_use]
    pub fn for_tenant(&self, tenant: Tenant) -> Self {
        Self {
            inner: self.inner.clone(),
            multi_tenant: self.multi_tenant,
            pinned: Some(tenant),
        }
    }

    /// The tenant a collection opened right now would be confined to.
    #[must_use]
    pub fn effective_tenant(&self) -> Tenant {
        if let Some(t) = &self.pinned {
            t.clone()
        } else if self.multi_tenant {
            current_tenant().unwrap_or(Tenant::Deny)
        } else {
            Tenant::Global
        }
    }

    #[must_use]
    pub fn collection<T: Send + Sync>(&self, name: &str) -> ScopedCollection<T> {
        let tenant = self.effective_tenant();
        // Only the multi-tenant cloud publishes a change log; single-shop
        // Mongo deployments have no sync and keep their documents untouched.
        let origin = (tenant.is_scoped() && is_synced_collection(name))
            .then(|| current_origin().unwrap_or_else(|| Arc::from(CLOUD_ORIGIN)));
        ScopedCollection {
            inner: self.inner.collection::<T>(name),
            tenant,
            origin,
        }
    }

    /// A collection that is never tenant-filtered, for platform-level data that
    /// belongs to no tenant (the shop-code directory in `modules::tenants`).
    /// Never use it for tenant-owned collections.
    #[must_use]
    pub fn platform_collection<T: Send + Sync>(&self, name: &str) -> ScopedCollection<T> {
        ScopedCollection {
            inner: self.inner.collection::<T>(name),
            tenant: Tenant::Global,
            origin: None,
        }
    }

    /// The unscoped driver handle, for admin work only (index creation, test
    /// cleanup, one-off maintenance binaries). Repositories must go through
    /// `collection`; CI greps for this name outside `clients/`, `bin/` and tests.
    #[must_use]
    pub fn unscoped(&self) -> &Database {
        &self.inner
    }

    /// The driver client, only to open a session for a transaction (`Db::begin`).
    pub(crate) fn client(&self) -> &mongodb::Client {
        self.inner.client()
    }
}
