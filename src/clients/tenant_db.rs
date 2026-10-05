// Tenant-aware MongoDB handle. `TenantDatabase` is what `Db::Mongo` holds and
// `ScopedCollection` is what repositories get from `.collection(..)`: every
// read, write and aggregation is rewritten through `core::tenancy` before it
// reaches the driver, so a repository cannot forget the tenant filter. With
// `Tenant::Global` the rewrites are identity, i.e. the raw driver behaviour.
//
// Writes to a synced collection (`core::sync_origin::SYNCED_COLLECTIONS`) on a
// multi-tenant handle are also stamped with the request's origin
// (`updated_by_device`), so the sync change log always names the real writer.
//
// Deliberately NOT here: index creation and other admin operations. Those need
// the raw handle (`TenantDatabase::unscoped`).

use std::{borrow::Borrow, sync::Arc};

use mongodb::{
    Collection, Cursor, Database,
    action::{Distinct, Find, FindOne, FindOneAndDelete, FindOneAndUpdate, Update as UpdateAction},
    bson::{Bson, Document},
    error::Error as MongoError,
    options::UpdateModifications,
    results::{DeleteResult, InsertOneResult},
};
use serde::{Serialize, de::DeserializeOwned};

use crate::core::{
    sync_origin::{self, CLOUD_ORIGIN, current_origin, is_synced_collection, stamp_update},
    tenancy::{Tenant, current_tenant, scope_filter, scope_pipeline, stamp_document},
};

#[derive(Clone)]
pub struct TenantDatabase {
    inner: Database,
    /// Multi-tenant handles resolve the tenant when a collection is opened;
    /// single-tenant ones never filter.
    multi_tenant: bool,
    /// Set by `for_tenant`; wins over the ambient request tenant.
    pinned: Option<Tenant>,
}

impl TenantDatabase {
    /// Unscoped handle (single-shop deployments and platform collections).
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
    pub fn multi_tenant(inner: Database) -> Self {
        Self {
            inner,
            multi_tenant: true,
            pinned: None,
        }
    }

    /// Same database, permanently confined to one tenant.
    pub fn for_tenant(&self, tenant: Tenant) -> Self {
        Self {
            inner: self.inner.clone(),
            multi_tenant: self.multi_tenant,
            pinned: Some(tenant),
        }
    }

    /// The tenant a collection opened right now would be confined to.
    pub fn effective_tenant(&self) -> Tenant {
        if let Some(t) = &self.pinned {
            t.clone()
        } else if self.multi_tenant {
            current_tenant().unwrap_or(Tenant::Deny)
        } else {
            Tenant::Global
        }
    }

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
    pub fn unscoped(&self) -> &Database {
        &self.inner
    }
}

pub struct ScopedCollection<T: Send + Sync> {
    inner: Collection<T>,
    tenant: Tenant,
    /// Writer stamped on every write (`updated_by_device`); `None` leaves
    /// documents untouched (unsynced collection, single-tenant handle, or
    /// `preserve_origin`).
    origin: Option<Arc<str>>,
}

impl<T: Send + Sync> ScopedCollection<T> {
    /// Same collection, but writes keep whatever `updated_by_device` the
    /// documents already have. Only for writes that are a consequence of
    /// another device's change rather than a change of their own (derived
    /// ledger totals recomputed after a push).
    pub fn preserve_origin(mut self) -> Self {
        self.origin = None;
        self
    }

    fn stamp(&self, update: impl Into<UpdateModifications>) -> UpdateModifications {
        match &self.origin {
            Some(origin) => stamp_update(origin, update.into()),
            None => update.into(),
        }
    }

    fn stamp_doc(&self, document: Document) -> Document {
        let document = stamp_document(&self.tenant, document);
        match &self.origin {
            Some(origin) => sync_origin::stamp_document(origin, document),
            None => document,
        }
    }
}

impl<T: DeserializeOwned + Send + Sync> ScopedCollection<T> {
    pub fn find(&self, filter: Document) -> Find<'_, T> {
        self.inner.find(scope_filter(&self.tenant, filter))
    }

    pub fn find_one(&self, filter: Document) -> FindOne<'_, T> {
        self.inner.find_one(scope_filter(&self.tenant, filter))
    }

    pub fn find_one_and_update(
        &self,
        filter: Document,
        update: impl Into<UpdateModifications>,
    ) -> FindOneAndUpdate<'_, T> {
        // An upsert copies the equality conditions of the filter into the new
        // document, so a scoped upsert is created inside the tenant as well.
        self.inner
            .find_one_and_update(scope_filter(&self.tenant, filter), self.stamp(update))
    }

    pub fn find_one_and_delete(&self, filter: Document) -> FindOneAndDelete<'_, T> {
        self.inner
            .find_one_and_delete(scope_filter(&self.tenant, filter))
    }

    pub fn update_one(
        &self,
        filter: Document,
        update: impl Into<UpdateModifications>,
    ) -> UpdateAction<'_> {
        self.inner
            .update_one(scope_filter(&self.tenant, filter), self.stamp(update))
    }

    pub fn update_many(
        &self,
        filter: Document,
        update: impl Into<UpdateModifications>,
    ) -> UpdateAction<'_> {
        self.inner
            .update_many(scope_filter(&self.tenant, filter), self.stamp(update))
    }

    pub async fn delete_one(&self, filter: Document) -> Result<DeleteResult, MongoError> {
        self.inner
            .delete_one(scope_filter(&self.tenant, filter))
            .await
    }

    pub async fn delete_many(&self, filter: Document) -> Result<DeleteResult, MongoError> {
        self.inner
            .delete_many(scope_filter(&self.tenant, filter))
            .await
    }

    pub async fn count_documents(&self, filter: Document) -> Result<u64, MongoError> {
        self.inner
            .count_documents(scope_filter(&self.tenant, filter))
            .await
    }

    pub fn distinct(&self, field: impl AsRef<str>, filter: Document) -> Distinct<'_> {
        self.inner
            .distinct(field.as_ref(), scope_filter(&self.tenant, filter))
    }

    /// Runs the pipeline confined to the tenant. A stage that cannot be scoped
    /// safely (see `core::tenancy`) fails the call instead of running unscoped.
    pub async fn aggregate(
        &self,
        pipeline: impl IntoIterator<Item = Document>,
    ) -> Result<Cursor<Document>, MongoError> {
        let scoped = scope_pipeline(&self.tenant, pipeline.into_iter().collect())
            .map_err(|e| MongoError::custom(e.to_string()))?;
        self.inner.aggregate(scoped).await
    }

    /// Same tenant confinement as [`Self::aggregate`], with a collation applied
    /// (e.g. case-insensitive `$sort` on text fields).
    pub async fn aggregate_with_collation(
        &self,
        pipeline: impl IntoIterator<Item = Document>,
        collation: mongodb::options::Collation,
    ) -> Result<Cursor<Document>, MongoError> {
        let scoped = scope_pipeline(&self.tenant, pipeline.into_iter().collect())
            .map_err(|e| MongoError::custom(e.to_string()))?;
        self.inner.aggregate(scoped).collation(collation).await
    }
}

impl<T: Serialize + DeserializeOwned + Send + Sync> ScopedCollection<T> {
    /// Inserts the document stamped with the tenant (any caller-supplied
    /// `tenant_id` is overwritten) and, on a synced collection, the origin.
    pub async fn insert_one(&self, doc: impl Borrow<T>) -> Result<InsertOneResult, MongoError> {
        let mut document = match mongodb::bson::serialize_to_bson(doc.borrow())? {
            Bson::Document(d) => d,
            _ => return Err(MongoError::custom("inserted value is not a document")),
        };
        document = self.stamp_doc(document);
        self.inner
            .clone_with_type::<Document>()
            .insert_one(document)
            .await
    }

    /// Replaces the matching document, keeping it inside the tenant.
    pub async fn replace_one(
        &self,
        filter: Document,
        replacement: impl Borrow<T>,
    ) -> Result<mongodb::results::UpdateResult, MongoError> {
        let mut document = match mongodb::bson::serialize_to_bson(replacement.borrow())? {
            Bson::Document(d) => d,
            _ => return Err(MongoError::custom("replacement is not a document")),
        };
        document = self.stamp_doc(document);
        self.inner
            .clone_with_type::<Document>()
            .replace_one(scope_filter(&self.tenant, filter), document)
            .await
    }
}
