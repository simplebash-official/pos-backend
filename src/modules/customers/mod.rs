pub mod model;
mod repository;
pub mod routes;
// `pub(crate)` rather than private: `modules::sync` calls
// `service::hydrate_sync_documents` to convert raw customer documents into
// the same DTO the REST reads return. Still unreachable from outside the
// crate, so `routes` remains the only public entry point.
pub(crate) mod service;
