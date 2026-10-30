// HTTP surface of cloud sync v2 (multi-tenant MongoDB deployments only):
// `POST /push`, `GET /pull`, `GET /snapshot`, `POST /devices/register` and
// `POST /setup-complete`, plus `GET /events` (realtime change notifications).
// All but `/events` require a registered-device token (`CurrentUser::require_device`); a web
// owner session cannot call them - it uses the normal REST API - and a revoked
// device is refused with 403 `DEVICE_REVOKED`. On a single-shop deployment
// (SQLite desktop) they answer 404: those backends are the *clients* of this
// protocol, not its server.

mod change_events;
mod get_snapshot;
mod mark_setup_complete;
mod pull_changes;
mod pull_query;
mod push_changes;
mod register_device;
mod register_device_request;
mod registered_device;
mod require_cloud;
mod router;
mod setup_complete_request;
mod setup_complete_response;
mod snapshot_query;

pub use pull_query::PullQuery;
pub use register_device_request::RegisterDeviceRequest;
pub use registered_device::RegisteredDevice;
pub use router::router;
pub use setup_complete_request::SetupCompleteRequest;
pub use setup_complete_response::SetupCompleteResponse;
pub use snapshot_query::SnapshotQuery;
