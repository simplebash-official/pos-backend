// Drawer cash categories: the data-driven list a cash movement is filed under,
// per movement type. Shops grow it on the spot; managers rename, merge, hide and
// review it. `repository`/`service` follow the same private-submodule layering as
// `suppliers`; `model` stays `pub` so tests can seed fixtures directly.
pub mod migration;
pub mod model;
mod repository;
pub mod routes;
pub mod service;
