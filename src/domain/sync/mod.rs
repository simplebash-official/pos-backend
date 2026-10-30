//! Request and response types of the sync status and changes endpoints.

mod domain_sync;

pub use domain_sync::ResourceChanges;
pub use domain_sync::ResourceSyncStatus;
pub use domain_sync::SyncChangesQuery;
pub use domain_sync::SyncChangesResponse;
pub use domain_sync::SyncStatusResponse;
