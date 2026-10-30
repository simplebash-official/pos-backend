//! HTTP surface of cloud sync v2 (multi-tenant MongoDB deployments only):
//! `POST /push`, `GET /pull`, `GET /snapshot`, `POST /devices/register` and
//! `POST /setup-complete`, plus `GET /events` (realtime change notifications).
//! All but `/events` require a registered-device token (`CurrentUser::require_device`); a web
//! owner session cannot call them - it uses the normal REST API - and a revoked
//! device is refused with 403 `DEVICE_REVOKED`. On a single-shop deployment
//! (SQLite desktop) they answer 404: those backends are the *clients* of this
//! protocol, not its server.

mod sync_routes_v2;

pub use sync_routes_v2::PullQuery;
pub use sync_routes_v2::RegisterDeviceRequest;
pub use sync_routes_v2::RegisteredDevice;
pub use sync_routes_v2::SetupCompleteRequest;
pub use sync_routes_v2::SetupCompleteResponse;
pub use sync_routes_v2::SnapshotQuery;
pub use sync_routes_v2::router;
