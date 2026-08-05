// Login, "who am I", and login-session history — token issuance only.
// `auth` owns the `login_sessions` collection (its own audit log, see
// `model.rs`/`repository.rs`) but never touches the `users` collection
// directly: account data is reached exclusively through
// `users::service::verify_credentials`/`get_user` (`pub` on that module's
// `service`), the same "reach another module through its `service`, never
// its `repository`" rule `supplier_products`/`purchases` already follow for
// `suppliers`/`inventory`. `model`/`repository`/`service` are private so
// nothing outside this module tree can reach past `routes`.
mod model;
mod repository;
pub mod routes;
mod service;
