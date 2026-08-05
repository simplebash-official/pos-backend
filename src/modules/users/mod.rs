// User account CRUD and role management — admin-provisioned only (see
// `service::create_user`; there is no public self-registration route).
// `model`/`repository` are private so nothing outside this module tree can
// reach past `routes`/`service` into persistence (see `modules::inventory`
// for the reference pattern this mirrors); `service` is `pub` (not
// `pub(crate)`) so `modules::auth` can reach `verify_credentials`/`get_user`
// and `src/bin/seed_admin.rs` can reach `create_user`.
mod model;
mod repository;
pub mod routes;
pub mod service;
