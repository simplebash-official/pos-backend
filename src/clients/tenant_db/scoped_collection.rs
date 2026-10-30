use crate::core::sync_origin;
use crate::core::sync_origin::stamp_update;
use crate::core::tenancy::{Tenant, scope_filter, scope_pipeline, stamp_document};
use mongodb::action::{
    Distinct, Find, FindOne, FindOneAndDelete, FindOneAndUpdate, Update as UpdateAction,
};
use mongodb::bson::{Bson, Document};
use mongodb::error::Error as MongoError;
use mongodb::options::UpdateModifications;
use mongodb::results::{DeleteResult, InsertOneResult};
use mongodb::{Collection, Cursor};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::borrow::Borrow;
use std::sync::Arc;

#[derive(Debug)]
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
