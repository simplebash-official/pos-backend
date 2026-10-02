pub mod apply;
pub mod blocks;
pub mod cursor;
pub mod derived_sqlite;
pub mod outbox;
mod repository;

// Cloud (MongoDB, multi-tenant) side of sync v2.
pub mod apply_mongo;
pub mod cloud_capture;
pub(crate) mod cloud_store;
pub mod compaction;
pub(crate) mod derived_mongo;
pub(crate) mod mongo_codec;
pub mod pull;
pub mod push;
pub mod resources;
pub mod routes;
pub mod routes_local;
pub mod routes_v2;
pub mod service;
pub mod snapshot;
pub mod state;

use crate::{
    core::error::{AppError, AppResult},
    domain::sync_v2::ApplyOutcome,
};

/// Placeholder body for the `Db::Mongo` arm of a per-module
/// `apply_sync_documents` until the cloud package fills it in.
#[allow(dead_code)]
pub(crate) fn unimplemented_mongo() -> AppResult<ApplyOutcome> {
    Err(AppError::internal(
        "sync apply is not implemented for MongoDB yet",
    ))
}
