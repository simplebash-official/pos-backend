//! Request/response types of the device-local sync API (`/api/sync/state`,
//! `/outbox`, `/apply`, `/blocks`, `/conflicts`). Only the desktop shell's sync
//! agent calls these, with a service token. Pure data, camelCase on the wire.

mod domain_sync_local;

pub use domain_sync_local::ConflictItem;
pub use domain_sync_local::EnableRequest;
pub use domain_sync_local::EnableResponse;
pub use domain_sync_local::NumberBlockInfo;
pub use domain_sync_local::OutboxAckRequest;
pub use domain_sync_local::OutboxItem;
pub use domain_sync_local::OutboxQuery;
pub use domain_sync_local::OutboxResponse;
pub use domain_sync_local::PendingItem;
pub use domain_sync_local::PendingQuery;
pub use domain_sync_local::PendingResponse;
pub use domain_sync_local::ResolveConflictRequest;
pub use domain_sync_local::ResourceCount;
pub use domain_sync_local::SeedResponse;
pub use domain_sync_local::SkuPrefixesResponse;
pub use domain_sync_local::StoreBlockRequest;
pub use domain_sync_local::SyncStateResponse;
pub use domain_sync_local::UpdateSyncStateRequest;
