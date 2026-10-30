// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). `login` is the one public (unauthenticated) route in
// this file; `me`/`sessions` require `CurrentUser`, and `sessions`
// additionally checks `require_permission(SESSIONS_VIEW)` as the first line
// of its body (see `core::middleware::auth::CurrentUser` for why that's a
// method call rather than a dedicated extractor type). The placeholder
// `GET /` status route this module used to have is gone — these are its
// real endpoints now, matching the precedent that `suppliers`/
// `supplier_products`/`purchases` don't carry one once real routes exist.

mod list_sessions;
mod login;
mod lookup_shop;
mod me;
mod router;
mod set_pin;
mod update_my_preferences;
mod verify_pin;

pub use router::router;
