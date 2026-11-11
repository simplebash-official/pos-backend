use crate::core::sync_origin;
use crate::core::sync_origin::{CLOUD_ORIGIN, current_origin, is_synced_collection, stamp_update};
use crate::core::tenancy::{Tenant, current_tenant, scope_filter, scope_pipeline, stamp_document};
use mongodb::action::{
    Distinct, Find, FindOne, FindOneAndDelete, FindOneAndUpdate, Update as UpdateAction,
};
use mongodb::bson::{Bson, Document};
use mongodb::error::Error as MongoError;
use mongodb::options::UpdateModifications;
use mongodb::results::{DeleteResult, InsertOneResult};
use mongodb::{Collection, Cursor, Database};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::borrow::Borrow;
use std::sync::Arc;

#[derive(Debug)]
/// A collection whose every read and write is confined to the current tenant.
pub struct ScopedCollection<T: Send + Sync> {
    pub(super) inner: Collection<T>,
    pub(super) tenant: Tenant,
    /// Writer stamped on every write (`updated_by_device`); `None` leaves
    /// documents untouched (unsynced collection, single-tenant handle, or
    /// `preserve_origin`).
    pub(super) origin: Option<Arc<str>>,
}

impl<T: Send + Sync> ScopedCollection<T> {
    /// Same collection, but writes keep whatever `updated_by_device` the
    /// documents already have. Only for writes that are a consequence of
    /// another device's change rather than a change of their own (derived
    /// ledger totals recomputed after a push).
    #[must_use]
    pub fn preserve_origin(mut self) -> Self {
        self.origin = None;
        self
    }

    pub(super) fn stamp(&self, update: impl Into<UpdateModifications>) -> UpdateModifications {
        match &self.origin {
            Some(origin) => stamp_update(origin, update.into()),
            None => update.into(),
        }
    }

    pub(super) fn stamp_doc(&self, document: Document) -> Document {
        let document = stamp_document(&self.tenant, document);
        match &self.origin {
            Some(origin) => sync_origin::stamp_document(origin, document),
            None => document,
        }
    }
}

impl<T: DeserializeOwned + Send + Sync> ScopedCollection<T> {
    /// Finds the documents matching `filter` within the tenant.
    pub fn find(&self, filter: Document) -> Find<'_, T> {
        self.inner.find(scope_filter(&self.tenant, filter))
    }

    /// Collects all tenant documents matching `filter` inside a transaction's session when present.
    pub async fn find_all_in(
        &self,
        filter: Document,
        sort: Option<Document>,
        session: Option<&mut mongodb::ClientSession>,
    ) -> Result<Vec<T>, MongoError> {
        use futures_util::TryStreamExt;
        let mut find = self.find(filter);
        if let Some(sort) = sort {
            find = find.sort(sort);
        }
        match session {
            Some(session) => {
                let mut cursor = find.session(&mut *session).await?;
                let mut stream = cursor.stream(session);
                let mut out = Vec::new();
                while let Some(doc) = stream.try_next().await? {
                    out.push(doc);
                }
                Ok(out)
            }
            None => {
                let mut cursor = find.await?;
                let mut out = Vec::new();
                while let Some(doc) = cursor.try_next().await? {
                    out.push(doc);
                }
                Ok(out)
            }
        }
    }

    /// Finds the one; `None` when there is none.
    pub fn find_one(&self, filter: Document) -> FindOne<'_, T> {
        self.inner.find_one(scope_filter(&self.tenant, filter))
    }

    /// Finds the one and update; `None` when there is none.
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

    /// Finds the one and delete; `None` when there is none.
    pub fn find_one_and_delete(&self, filter: Document) -> FindOneAndDelete<'_, T> {
        self.inner
            .find_one_and_delete(scope_filter(&self.tenant, filter))
    }

    /// Updates the one.
    pub fn update_one(
        &self,
        filter: Document,
        update: impl Into<UpdateModifications>,
    ) -> UpdateAction<'_> {
        self.inner
            .update_one(scope_filter(&self.tenant, filter), self.stamp(update))
    }

    /// Updates the many.
    pub fn update_many(
        &self,
        filter: Document,
        update: impl Into<UpdateModifications>,
    ) -> UpdateAction<'_> {
        self.inner
            .update_many(scope_filter(&self.tenant, filter), self.stamp(update))
    }

    /// Deletes the one.
    pub async fn delete_one(&self, filter: Document) -> Result<DeleteResult, MongoError> {
        self.inner
            .delete_one(scope_filter(&self.tenant, filter))
            .await
    }

    /// Deletes the many.
    pub async fn delete_many(&self, filter: Document) -> Result<DeleteResult, MongoError> {
        self.inner
            .delete_many(scope_filter(&self.tenant, filter))
            .await
    }

    /// Counts the documents.
    pub async fn count_documents(&self, filter: Document) -> Result<u64, MongoError> {
        self.count_documents_in(filter, None).await
    }

    /// [`Self::count_documents`] inside a transaction's session when there is one.
    pub async fn count_documents_in(
        &self,
        filter: Document,
        session: Option<&mut mongodb::ClientSession>,
    ) -> Result<u64, MongoError> {
        let action = self
            .inner
            .count_documents(scope_filter(&self.tenant, filter));
        match session {
            Some(session) => action.session(session).await,
            None => action.await,
        }
    }

    /// The distinct values of `field` among the tenant's documents matching `filter`.
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
        self.insert_one_in(doc, None).await
    }

    /// [`Self::insert_one`] inside a transaction's session when there is one.
    pub async fn insert_one_in(
        &self,
        doc: impl Borrow<T>,
        session: Option<&mut mongodb::ClientSession>,
    ) -> Result<InsertOneResult, MongoError> {
        let mut document = match mongodb::bson::serialize_to_bson(doc.borrow())? {
            Bson::Document(d) => d,
            _ => return Err(MongoError::custom("inserted value is not a document")),
        };
        document = self.stamp_doc(document);
        let collection = self.inner.clone_with_type::<Document>();
        let action = collection.insert_one(document);
        match session {
            Some(session) => action.session(session).await,
            None => action.await,
        }
    }

    /// Replaces the matching document, keeping it inside the tenant.
    pub async fn replace_one(
        &self,
        filter: Document,
        replacement: impl Borrow<T>,
    ) -> Result<mongodb::results::UpdateResult, MongoError> {
        self.replace_one_in(filter, replacement, None).await
    }

    /// [`Self::replace_one`] inside a transaction's session when there is one.
    pub async fn replace_one_in(
        &self,
        filter: Document,
        replacement: impl Borrow<T>,
        session: Option<&mut mongodb::ClientSession>,
    ) -> Result<mongodb::results::UpdateResult, MongoError> {
        let mut document = match mongodb::bson::serialize_to_bson(replacement.borrow())? {
            Bson::Document(d) => d,
            _ => return Err(MongoError::custom("replacement is not a document")),
        };
        document = self.stamp_doc(document);
        let collection = self.inner.clone_with_type::<Document>();
        let action = collection.replace_one(scope_filter(&self.tenant, filter), document);
        match session {
            Some(session) => action.session(session).await,
            None => action.await,
        }
    }
}

#[derive(Debug, Clone)]
/// A MongoDB database handle that hands out tenant-scoped collections.
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
            match current_tenant() {
                Some(tenant) => tenant,
                // No tenant in scope on a multi-tenant server: nothing may be read or written.
                None => Tenant::Deny,
            }
        } else {
            Tenant::Global
        }
    }

    #[must_use]
    /// The collection `name`, scoped to the tenant in effect.
    pub fn collection<T: Send + Sync>(&self, name: &str) -> ScopedCollection<T> {
        let tenant = self.effective_tenant();
        // Only the multi-tenant cloud publishes a change log; single-shop
        // Mongo deployments have no sync and keep their documents untouched.
        let origin =
            (tenant.is_scoped() && is_synced_collection(name)).then(|| match current_origin() {
                Some(origin) => origin,
                // A request that names no device is made by the cloud itself.
                None => Arc::from(CLOUD_ORIGIN),
            });
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
