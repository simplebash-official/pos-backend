//! HTTP layer only: extractors, path/query parsing, and OpenAPI docs
//! (`#[utoipa::path]`). Every route calls
//! `user.require_any_permission(&[USERS_MANAGE, USERS_MANAGE_STAFF])` as the
//! first line of the handler body — see `core::middleware::auth::CurrentUser`
//! for why this is a method call rather than a dedicated extractor type —
//! then passes the caller's role into `service::*`, which enforces the
//! actual Admin/Manager management hierarchy (an Admin manages Manager and
//! Staff accounts, a Manager manages Staff accounts only — see
//! `modules::users::service::manageable_roles`). Unlike most other modules,
//! there's no placeholder `GET /` status route here — the collection root
//! (`GET /`, nested at `/api/users`) is itself the real "list users"
//! endpoint from day one.

mod users_routes;

pub use users_routes::router;
